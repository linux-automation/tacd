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
