use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "stub_out_dbus"))]
pub use futures_lite::future::race;

#[cfg(not(feature = "stub_out_dbus"))]
pub use log::warn;

use super::{Connection, Result};
use crate::broker::{BrokerBuilder, Topic};

#[cfg(not(feature = "stub_out_dbus"))]
mod manager;

#[cfg(not(feature = "stub_out_dbus"))]
mod service;

#[derive(Serialize, Deserialize)]
pub struct ServiceStatus {
    pub active_state: String,
    pub sub_state: String,
    pub active_enter_ts: u64,
    pub active_exit_ts: u64,
}

#[derive(Serialize, Deserialize)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}

#[derive(Clone)]
pub struct Service {
    pub action: Arc<Topic<ServiceAction>>,
    pub status: Arc<Topic<ServiceStatus>>,
}

#[derive(Clone)]
pub struct Systemd {
    pub reboot: Arc<Topic<bool>>,
    pub networkmanager: Service,
    pub labgrid: Service,
    pub iobus: Service,
}

impl ServiceStatus {
    #[cfg(feature = "stub_out_dbus")]
    async fn get() -> Result<Self> {
        Ok(Self {
            active_state: "actvive".to_string(),
            sub_state: "running".to_string(),
            active_enter_ts: 0,
            active_exit_ts: 0,
        })
    }

    #[cfg(not(feature = "stub_out_dbus"))]
    async fn get<'a>(unit: &service::UnitProxy<'a>) -> Result<Self> {
        Ok(Self {
            active_state: unit.active_state().await?,
            sub_state: unit.sub_state().await?,
            active_enter_ts: unit.active_enter_timestamp().await?,
            active_exit_ts: unit.active_exit_timestamp().await?,
        })
    }
}

impl Service {
    fn setup_topics(bb: &mut BrokerBuilder, topic_name: &'static str) -> Self {
        Self {
            action: bb.topic_wo(&format!("/v1/tac/service/{topic_name}/action"), None),
            status: bb.topic_ro(&format!("/v1/tac/service/{topic_name}/status"), None),
        }
    }

    #[cfg(feature = "stub_out_dbus")]
    async fn new(
        bb: &mut BrokerBuilder,
        _conn: Arc<Connection>,
        topic_name: &'static str,
        _unit_name: &'static str,
    ) -> Self {
        let this = Self::setup_topics(bb, topic_name);

        this.status.set(ServiceStatus::get().await.unwrap()).await;

        this
    }

    #[cfg(not(feature = "stub_out_dbus"))]
    async fn new(
        bb: &mut BrokerBuilder,
        conn: Arc<Connection>,
        topic_name: &'static str,
        unit_name: &'static str,
    ) -> Self {
        let this = Self::setup_topics(bb, topic_name);

        let unit_path = {
            let manager = manager::ManagerProxy::new(&conn).await.unwrap();
            manager.get_unit(unit_name).await.unwrap()
        };

        let unit = service::UnitProxy::builder(&conn)
            .path(unit_path)
            .unwrap()
            .build()
            .await
            .unwrap();

        let mut active_state_stream = unit.receive_active_state_changed().await.map(|_| ());
        let mut sub_state_stream = unit.receive_sub_state_changed().await.map(|_| ());
        let mut active_enter_stream = unit
            .receive_active_enter_timestamp_changed()
            .await
            .map(|_| ());
        let mut active_exit_stream = unit
            .receive_active_exit_timestamp_changed()
            .await
            .map(|_| ());

        let unit_task = unit.clone();
        let status_topic = this.status.clone();

        spawn(async move {
            loop {
                let status = ServiceStatus::get(&unit_task).await.unwrap();
                status_topic.set(status).await;

                race(
                    race(active_state_stream.next(), sub_state_stream.next()),
                    race(active_enter_stream.next(), active_exit_stream.next()),
                )
                .await
                .unwrap();
            }
        });

        let (mut action_reqs, _) = this.action.clone().subscribe_unbounded().await;

        spawn(async move {
            while let Some(action) = action_reqs.next().await {
                let res = match *action {
                    ServiceAction::Start => unit.start("replace").await,
                    ServiceAction::Stop => unit.stop("replace").await,
                    ServiceAction::Restart => unit.restart("replace").await,
                };

                if let Err(e) = res {
                    warn!(
                        "Failed to perform action on systemd service {}: {}",
                        unit_name, e
                    );
                }
            }
        });

        this
    }
}

impl Systemd {
    #[cfg(feature = "stub_out_dbus")]
    pub async fn handle_reboot(reboot: Arc<Topic<bool>>, _conn: Arc<Connection>) {
        let (mut reboot_reqs, _) = reboot.subscribe_unbounded().await;

        spawn(async move {
            while let Some(req) = reboot_reqs.next().await {
                if *req {
                    println!("Asked to reboot but don't feel like it");
                }
            }
        });
    }

    #[cfg(not(feature = "stub_out_dbus"))]
    pub async fn handle_reboot(reboot: Arc<Topic<bool>>, conn: Arc<Connection>) {
        let (mut reboot_reqs, _) = reboot.subscribe_unbounded().await;

        spawn(async move {
            let manager = manager::ManagerProxy::new(&conn).await.unwrap();

            while let Some(req) = reboot_reqs.next().await {
                if *req {
                    if let Err(e) = manager.reboot().await {
                        warn!("Failed to trigger reboot: {}", e);
                    }
                }
            }
        });
    }

    pub async fn new(bb: &mut BrokerBuilder, conn: &Arc<Connection>) -> Self {
        let reboot = bb.topic_rw("/v1/tac/reboot", Some(false));

        Self::handle_reboot(reboot.clone(), conn.clone()).await;

        Self {
            reboot,
            networkmanager: Service::new(
                bb,
                conn.clone(),
                "network-manager",
                "NetworkManager.service",
            )
            .await,
            labgrid: Service::new(
                bb,
                conn.clone(),
                "labgrid-exporter",
                "labgrid-exporter.service",
            )
            .await,
            iobus: Service::new(bb, conn.clone(), "lxa-iobus", "lxa-iobus.service").await,
        }
    }
}
