use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use zbus::Connection;

use log::warn;

use crate::broker::{BrokerBuilder, Topic};

mod manager;

#[derive(Clone)]
pub struct Systemd {
    pub restart_service: Arc<Topic<String>>,
    pub reboot: Arc<Topic<bool>>,
}

impl Systemd {
    fn setup_topics(bb: &mut BrokerBuilder) -> Self {
        Self {
            restart_service: bb.topic_rw("/v1/tac/restart_service", Some("".to_string())),
            reboot: bb.topic_rw("/v1/tac/reboot", Some(false)),
        }
    }

    #[cfg(feature = "stub_out_dbus")]
    pub async fn new<C>(bb: &mut BrokerBuilder, _conn: C) -> Self {
        Self::setup_topics(bb)
    }

    #[cfg(not(feature = "stub_out_dbus"))]
    pub async fn new(bb: &mut BrokerBuilder, conn: &Arc<Connection>) -> Self {
        let this = Self::setup_topics(bb);

        let (mut restart_reqs, _) = this.restart_service.clone().subscribe_unbounded().await;
        let conn_task = conn.clone();

        spawn(async move {
            let manager = manager::ManagerProxy::new(&conn_task).await.unwrap();

            while let Some(name) = restart_reqs.next().await {
                if let Err(e) = manager.restart_unit(&name, "replace").await {
                    warn!("Failed to restart systemd service {}: {}", name, e);
                }
            }
        });

        let (mut reboot_reqs, _) = this.reboot.clone().subscribe_unbounded().await;
        let conn_task = conn.clone();

        spawn(async move {
            let manager = manager::ManagerProxy::new(&conn_task).await.unwrap();

            while let Some(req) = reboot_reqs.next().await {
                if *req {
                    if let Err(e) = manager.reboot().await {
                        warn!("Failed to trigger reboot: {}", e);
                    }
                }
            }
        });

        this
    }
}
