// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2022 Pengutronix e.K.
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use async_std::prelude::*;
use async_std::sync::Arc;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "demo_mode"))]
use futures_lite::future::race;

#[cfg(not(feature = "demo_mode"))]
pub use log::warn;

use super::{Connection, Result};
use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(not(feature = "demo_mode"))]
mod manager;

#[cfg(not(feature = "demo_mode"))]
mod service;

#[derive(Serialize, Deserialize, Clone)]
pub struct ServiceStatus {
    pub active_state: String,
    pub sub_state: String,
    pub active_enter_ts: u64,
    pub active_exit_ts: u64,
}

#[derive(Serialize, Deserialize, Clone)]
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
    #[cfg(feature = "demo_mode")]
    async fn get() -> Result<Self> {
        Ok(Self {
            active_state: "active".to_string(),
            sub_state: "running".to_string(),
            active_enter_ts: 0,
            active_exit_ts: 0,
        })
    }

    #[cfg(not(feature = "demo_mode"))]
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
    fn new(bb: &mut BrokerBuilder, topic_name: &'static str) -> Self {
        Self {
            action: bb.topic_wo(&format!("/v1/tac/service/{topic_name}/action"), None),
            status: bb.topic_ro(&format!("/v1/tac/service/{topic_name}/status"), None),
        }
    }

    #[cfg(feature = "demo_mode")]
    async fn connect(
        &self,
        _wtb: &mut WatchedTasksBuilder,
        _conn: Arc<Connection>,
        _unit_name: &str,
    ) -> anyhow::Result<()> {
        self.status.set(ServiceStatus::get().await.unwrap());

        Ok(())
    }

    #[cfg(not(feature = "demo_mode"))]
    async fn connect(
        &self,
        wtb: &mut WatchedTasksBuilder,
        conn: Arc<Connection>,
        unit_name: &'static str,
    ) -> anyhow::Result<()> {
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

        let unit_task = unit.clone();
        let status_topic = self.status.clone();

        wtb.spawn_task(format!("systemd-{unit_name}-state"), async move {
            let mut active_state_stream =
                unit_task.receive_active_state_changed().await.map(|_| ());
            let mut sub_state_stream = unit_task.receive_sub_state_changed().await.map(|_| ());
            let mut active_enter_stream = unit_task
                .receive_active_enter_timestamp_changed()
                .await
                .map(|_| ());
            let mut active_exit_stream = unit_task
                .receive_active_exit_timestamp_changed()
                .await
                .map(|_| ());

            loop {
                let status = ServiceStatus::get(&unit_task).await.unwrap();
                status_topic.set(status);

                race(
                    race(active_state_stream.next(), sub_state_stream.next()),
                    race(active_enter_stream.next(), active_exit_stream.next()),
                )
                .await
                .unwrap();
            }
        })?;

        let (mut action_reqs, _) = self.action.clone().subscribe_unbounded();

        wtb.spawn_task(format!("systemd-{unit_name}-actions"), async move {
            while let Some(action) = action_reqs.next().await {
                let res = match action {
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

            Ok(())
        })?;

        Ok(())
    }
}

impl Systemd {
    #[cfg(feature = "demo_mode")]
    pub fn handle_reboot(
        wtb: &mut WatchedTasksBuilder,
        reboot: Arc<Topic<bool>>,
        _conn: Arc<Connection>,
    ) -> anyhow::Result<()> {
        let (mut reboot_reqs, _) = reboot.subscribe_unbounded();

        wtb.spawn_task("systemd-reboot", async move {
            while let Some(req) = reboot_reqs.next().await {
                if req {
                    println!("Asked to reboot but don't feel like it");
                }
            }

            Ok(())
        })
    }

    #[cfg(not(feature = "demo_mode"))]
    pub fn handle_reboot(
        wtb: &mut WatchedTasksBuilder,
        reboot: Arc<Topic<bool>>,
        conn: Arc<Connection>,
    ) -> anyhow::Result<()> {
        let (mut reboot_reqs, _) = reboot.subscribe_unbounded();

        wtb.spawn_task("systemd-reboot", async move {
            let manager = manager::ManagerProxy::new(&conn).await.unwrap();

            while let Some(req) = reboot_reqs.next().await {
                if req {
                    if let Err(e) = manager.reboot().await {
                        warn!("Failed to trigger reboot: {}", e);
                    }
                }
            }

            Ok(())
        })
    }

    pub async fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        conn: &Arc<Connection>,
    ) -> anyhow::Result<Self> {
        let reboot = bb.topic_rw("/v1/tac/reboot", Some(false));

        Self::handle_reboot(wtb, reboot.clone(), conn.clone())?;

        let networkmanager = Service::new(bb, "network-manager");
        let labgrid = Service::new(bb, "labgrid-exporter");
        let iobus = Service::new(bb, "lxa-iobus");

        networkmanager
            .connect(wtb, conn.clone(), "NetworkManager.service")
            .await?;
        labgrid
            .connect(wtb, conn.clone(), "labgrid-exporter.service")
            .await?;
        iobus
            .connect(wtb, conn.clone(), "lxa-iobus.service")
            .await?;

        Ok(Self {
            reboot,
            networkmanager,
            labgrid,
            iobus,
        })
    }
}
