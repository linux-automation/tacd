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

use std::time::Duration;

use anyhow::Result;
use async_std::sync::Arc;
use async_std::task::sleep;

use serde::{Deserialize, Serialize};

use crate::adc::CalibratedChannel;
use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

const CURRENT_MAX: f32 = 0.2;
const VOLTAGE_MIN: f32 = 10.0;

#[cfg(feature = "demo_mode")]
mod http {
    use super::{LSSState, Nodes, ServerInfo};

    pub(super) struct RequestDecoy {}

    pub(super) trait DemoModeDefault {
        fn demo_get() -> Self;
    }

    impl DemoModeDefault for ServerInfo {
        fn demo_get() -> Self {
            Self {
                hostname: "lxatac-1000".to_string(),
                started: "some time ago".to_string(),
                can_interface: "can0".to_string(),
                can_interface_is_up: true,
                lss_state: LSSState::Idle,
                can_tx_error: false,
            }
        }
    }

    impl DemoModeDefault for Nodes {
        fn demo_get() -> Self {
            Self {
                code: 0,
                error_message: "".to_string(),
                result: Vec::new(),
            }
        }
    }

    impl RequestDecoy {
        pub(super) async fn recv_json<T: DemoModeDefault>(&self) -> Result<T, ()> {
            Ok(T::demo_get())
        }
    }

    pub(super) fn get(_: &str) -> RequestDecoy {
        RequestDecoy {}
    }
}

#[cfg(not(feature = "demo_mode"))]
mod http {
    pub(super) use surf::get;
}

#[derive(PartialEq, Serialize, Deserialize, Debug, Clone)]
pub struct Nodes {
    pub code: u32,
    pub error_message: String,
    pub result: Vec<String>,
}

#[derive(PartialEq, Serialize, Deserialize, Debug, Clone)]
pub enum LSSState {
    Idle,
    Scanning,
}

#[derive(PartialEq, Serialize, Deserialize, Debug, Clone)]
pub struct ServerInfo {
    pub hostname: String,
    pub started: String,
    pub can_interface: String,
    pub can_interface_is_up: bool,
    pub lss_state: LSSState,
    pub can_tx_error: bool,
}

pub struct IoBus {
    pub supply_fault: Arc<Topic<bool>>,
    pub server_info: Arc<Topic<ServerInfo>>,
    pub nodes: Arc<Topic<Nodes>>,
}

impl IoBus {
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        iobus_pwr_en: Arc<Topic<bool>>,
        iobus_curr: CalibratedChannel,
        iobus_volt: CalibratedChannel,
    ) -> Result<Self> {
        let supply_fault = bb.topic_ro("/v1/iobus/feedback/fault", None);
        let server_info = bb.topic_ro("/v1/iobus/server/info", None);
        let nodes = bb.topic_ro("/v1/iobus/server/nodes", None);

        let supply_fault_task = supply_fault.clone();
        let server_info_task = server_info.clone();
        let nodes_task = nodes.clone();

        wtb.spawn_task("iobus-update", async move {
            loop {
                if let Ok(si) = http::get("http://127.0.0.1:8080/server-info/")
                    .recv_json::<ServerInfo>()
                    .await
                {
                    server_info_task.set_if_changed(si);
                }

                if let Ok(nodes) = http::get("http://127.0.0.1:8080/nodes/")
                    .recv_json::<Nodes>()
                    .await
                {
                    nodes_task.set_if_changed(nodes);
                }

                // Report the power supply health
                let pwr_en = iobus_pwr_en.try_get().unwrap_or(false);
                let current = iobus_curr.get();
                let voltage = iobus_volt.get();

                if let (Ok(current), Ok(voltage)) = (current, voltage) {
                    let undervolt = pwr_en && (voltage.value < VOLTAGE_MIN);
                    let overcurrent = current.value > CURRENT_MAX;

                    supply_fault_task.set_if_changed(undervolt || overcurrent);
                }

                sleep(Duration::from_secs(1)).await;
            }
        })?;

        Ok(Self {
            supply_fault,
            server_info,
            nodes,
        })
    }
}
