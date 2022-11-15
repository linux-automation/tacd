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

use std::time::Duration;

use async_std::sync::Arc;
use async_std::task::{sleep, spawn};

use serde::{Deserialize, Serialize};
use surf;

use crate::broker::{BrokerBuilder, Topic};

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
    pub server_info: Arc<Topic<ServerInfo>>,
    pub nodes: Arc<Topic<Nodes>>,
}

impl IoBus {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        let server_info = bb.topic_ro("/v1/iobus/server/info", None);
        let nodes = bb.topic_ro("/v1/iobus/server/nodes", None);

        let server_info_task = server_info.clone();
        let nodes_task = nodes.clone();

        spawn(async move {
            loop {
                if let Ok(si) = surf::get("http://127.0.0.1:8080/server-info/")
                    .recv_json::<ServerInfo>()
                    .await
                {
                    server_info_task
                        .modify(|prev| {
                            let need_update = prev.map(|p| *p != si).unwrap_or(true);

                            if need_update {
                                Some(Arc::new(si))
                            } else {
                                None
                            }
                        })
                        .await;
                }

                if let Ok(nodes) = surf::get("http://127.0.0.1:8080/nodes/")
                    .recv_json::<Nodes>()
                    .await
                {
                    nodes_task
                        .modify(|prev| {
                            let need_update = prev.map(|n| *n != nodes).unwrap_or(true);

                            if need_update {
                                Some(Arc::new(nodes))
                            } else {
                                None
                            }
                        })
                        .await;
                }

                sleep(Duration::from_secs(1)).await;
            }
        });

        Self { server_info, nodes }
    }
}
