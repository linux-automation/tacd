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
// with this library; if not, see <https://www.gnu.org/licenses/>.

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
    unit_name: &'static str,
    #[cfg_attr(feature = "demo_mode", allow(dead_code))]
    pub action: Arc<Topic<ServiceAction>>,
    pub status: Arc<Topic<ServiceStatus>>,
}

#[derive(Clone)]
pub struct Systemd {
    pub reboot: Arc<Topic<bool>>,
    #[allow(dead_code)]
    pub networkmanager: Service,
    #[allow(dead_code)]
    pub labgrid: Service,
    #[allow(dead_code)]
    pub iobus: Service,
    pub rauc: Service,
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
    async fn get(unit: &service::UnitProxy<'_>) -> Result<Self> {
        Ok(Self {
            active_state: unit.active_state().await?,
            sub_state: unit.sub_state().await?,
            active_enter_ts: unit.active_enter_timestamp().await?,
            active_exit_ts: unit.active_exit_timestamp().await?,
        })
    }
}

pub enum ReloadError {
    Canceled,
    Timeout,
    Failed,
    Dependency,
    Skipped,
    DBus(zbus::Error),
}

impl From<&str> for ReloadError {
    fn from(value: &str) -> Self {
        match value {
            "canceled" => Self::Canceled,
            "timeout" => Self::Timeout,
            "failed" => Self::Failed,
            "dependency" => Self::Dependency,
            "skipped" => Self::Skipped,
            _ => Self::DBus(zbus::Error::Failure(format!("Unknown job result: {value}"))),
        }
    }
}

impl From<zbus::Error> for ReloadError {
    fn from(value: zbus::Error) -> Self {
        Self::DBus(value)
    }
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canceled => write!(f, "The reload job was canceled"),
            Self::Timeout => write!(f, "The reload job timed out"),
            Self::Failed => write!(f, "The reload job failed"),
            Self::Dependency => write!(f, "The reload job failed due to a dependency"),
            Self::Skipped => write!(f, "The reload job was skipped"),
            Self::DBus(e) => write!(f, "A DBus error occurred: {e}"),
        }
    }
}

impl Service {
    fn new(bb: &mut BrokerBuilder, unit_name: &'static str, topic_name: &'static str) -> Self {
        Self {
            unit_name,
            action: bb.topic_wo(&format!("/v1/tac/service/{topic_name}/action"), None),
            status: bb.topic_ro(&format!("/v1/tac/service/{topic_name}/status"), None),
        }
    }

    #[cfg(feature = "demo_mode")]
    async fn connect(
        &self,
        _wtb: &mut WatchedTasksBuilder,
        _conn: Arc<Connection>,
    ) -> anyhow::Result<()> {
        self.status.set(ServiceStatus::get().await.unwrap());

        Ok(())
    }

    #[cfg(not(feature = "demo_mode"))]
    async fn connect(
        &self,
        wtb: &mut WatchedTasksBuilder,
        conn: Arc<Connection>,
    ) -> anyhow::Result<()> {
        let unit_name = self.unit_name;
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

    #[cfg(feature = "demo_mode")]
    pub async fn reload(&self, _conn: &Connection) -> std::result::Result<(), ReloadError> {
        log::info!("Reloaded {}", self.unit_name);
        Ok(())
    }

    #[cfg(not(feature = "demo_mode"))]
    pub async fn reload(&self, conn: &Connection) -> std::result::Result<(), ReloadError> {
        let manager = manager::ManagerProxy::new(conn).await?;

        // According to the systemd dbus interface documentation the race-free
        // way to receive results for a Job (like restarting a service) is to
        // subscribe to JobRemoved signals before triggering the job and then
        // waiting for a removed job with the correct object path.

        // Subscribe to JobRemoved signals
        let mut job_removed_stream = manager.receive_job_removed().await?;

        // Trigger the reload and receive an object path as result
        let reload_job = manager
            .reload_or_restart_unit(self.unit_name, "replace")
            .await?;

        loop {
            // .next() returning None would mean the signal stream from systemd
            // ending, which should be an extremely unlikely scenario.
            let removed = job_removed_stream
                .next()
                .await
                .ok_or(zbus::Error::InvalidReply)?;

            let args = removed.args()?;

            if args.job == *reload_job {
                // This is the job we are looking for

                // The result is a string with values like "done", "failed", etc.
                // Convert that to a Result and exit.
                break match args.result {
                    "done" => Ok(()),
                    res => Err(res.into()),
                };
            }
        }
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
                if req && let Err(e) = manager.reboot().await {
                    warn!("Failed to trigger reboot: {}", e);
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

        let networkmanager = Service::new(bb, "NetworkManager.service", "network-manager");
        let labgrid = Service::new(bb, "labgrid-exporter.service", "labgrid-exporter");
        let iobus = Service::new(bb, "lxa-iobus.service", "lxa-iobus");
        let rauc = Service::new(bb, "rauc.service", "rauc");

        networkmanager.connect(wtb, conn.clone()).await?;
        labgrid.connect(wtb, conn.clone()).await?;
        iobus.connect(wtb, conn.clone()).await?;
        rauc.connect(wtb, conn.clone()).await?;

        Ok(Self {
            reboot,
            networkmanager,
            labgrid,
            iobus,
            rauc,
        })
    }
}
