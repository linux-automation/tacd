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
        let server_info = bb.topic_ro("/v1/iobus/server/info");
        let nodes = bb.topic_ro("/v1/iobus/server/nodes");

        let server_info_task = server_info.clone();
        let nodes_task = nodes.clone();

        spawn(async move {
            loop {
                if let Ok(si) = surf::get("http://127.0.0.1:8080/server-info/")
                    .recv_json::<ServerInfo>()
                    .await
                {
                    let need_update = server_info_task
                        .get()
                        .await
                        .map(|prev| *prev != si)
                        .unwrap_or(true);

                    if need_update {
                        server_info_task.set(si).await;
                    }
                }

                if let Ok(nodes) = surf::get("http://127.0.0.1:8080/nodes/")
                    .recv_json::<Nodes>()
                    .await
                {
                    let need_update = nodes_task
                        .get()
                        .await
                        .map(|prev| *prev != nodes)
                        .unwrap_or(true);

                    if need_update {
                        nodes_task.set(nodes).await;
                    }
                }

                sleep(Duration::from_secs(1)).await;
            }
        });

        Self { server_info, nodes }
    }
}
