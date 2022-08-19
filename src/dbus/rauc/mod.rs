use std::collections::HashMap;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use serde::{Deserialize, Serialize};
use zbus::Connection;

use crate::broker::{BrokerBuilder, Topic};

mod installer;

#[derive(Serialize, Deserialize)]
pub struct Progress {
    pub percentage: i32,
    pub message: String,
    pub nesting_depth: i32,
}

impl From<(i32, String, i32)> for Progress {
    fn from(p: (i32, String, i32)) -> Self {
        Self {
            percentage: p.0,
            message: p.1,
            nesting_depth: p.2,
        }
    }
}

type SlotStatus = HashMap<String, HashMap<String, String>>;

pub struct Rauc {
    pub operation: Arc<Topic<String>>,
    pub progress: Arc<Topic<Progress>>,
    pub slot_status: Arc<Topic<SlotStatus>>,
    pub last_error: Arc<Topic<String>>,
    pub install: Arc<Topic<String>>,
}

impl Rauc {
    pub async fn new(bb: &mut BrokerBuilder, conn: Arc<Connection>) -> Self {
        let inst = Self {
            operation: bb.topic_ro("/v1/tac/update/operation"),
            progress: bb.topic_ro("/v1/tac/update/progress"),
            slot_status: bb.topic_ro("/v1/tac/update/slots"),
            last_error: bb.topic_ro("/v1/tac/update/last_error"),
            install: bb.topic_wo("/v1/tac/update/install"),
        };

        let conn_task = conn.clone();
        let operation = inst.operation.clone();
        let slot_status = inst.slot_status.clone();

        spawn(async move {
            let proxy = installer::InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_operation_changed().await;

            if let Ok(v) = proxy.operation().await {
                operation.set(v).await;
            }

            loop {
                // Referesh the slot status whenever the current operation changes
                if let Ok(slots) = proxy.get_slot_status().await {
                    let slots = slots
                        .into_iter()
                        .map(|(slot_name, slot_info)| {
                            let mut info: HashMap<String, String> = slot_info
                                .into_iter()
                                .map(|(k, v)| {
                                    // Convert itegers to strings as raw zvariant values are
                                    // unusable when json serialized and I can not be bothered
                                    // to fiddle around with an enum that wraps strings and integers
                                    // or something like that
                                    let ss = v.downcast_ref::<str>().map(|s| s.to_string());
                                    let s32 = v.downcast_ref::<u32>().map(|i| format!("{i}"));
                                    let s64 = v.downcast_ref::<u64>().map(|i| format!("{i}"));

                                    let k = k
                                        .replace("type", "fs_type")
                                        .replace("class", "slot_class")
                                        .replace(".", "_")
                                        .replace("-", "_")
                                        .to_string();

                                    (k, ss.or(s32).or(s64).unwrap_or_else(|| String::new()))
                                })
                                .collect();

                            info.insert("name".to_string(), slot_name.clone());

                            (slot_name.replace(".", "_").to_string(), info)
                        })
                        .collect();

                    slot_status.set(slots).await;
                }

                // Wait for the current operation to change
                if let Some(v) = stream.next().await {
                    if let Ok(v) = v.get().await {
                        operation.set(v).await;
                    }
                } else {
                    break;
                }
            }
        });

        let conn_task = conn.clone();
        let progress = inst.progress.clone();

        spawn(async move {
            let proxy = installer::InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_progress_changed().await;

            if let Ok(p) = proxy.progress().await {
                progress.set(p.into()).await;
            }

            while let Some(v) = stream.next().await {
                if let Ok(p) = v.get().await {
                    progress.set(p.into()).await;
                }
            }
        });

        let conn_task = conn.clone();
        let last_error = inst.last_error.clone();

        spawn(async move {
            let proxy = installer::InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_last_error_changed().await;

            if let Ok(e) = proxy.last_error().await {
                last_error.set(e).await;
            }

            while let Some(v) = stream.next().await {
                if let Ok(e) = v.get().await {
                    last_error.set(e).await;
                }
            }
        });

        let conn_task = conn.clone();
        let install = inst.install.clone();

        spawn(async move {
            let proxy = installer::InstallerProxy::new(&conn_task).await.unwrap();
            let (mut stream, _) = install.subscribe_unbounded().await;

            while let Some(url) = stream.next().await {
                // Poor-mans validation. It feels wrong to let someone point to any
                // file on the TAC from the web interface.
                if url.starts_with("http://") || url.starts_with("https://") {
                    // TODO: some kind of error handling
                    let _ = proxy.install(&url).await;
                }
            }
        });

        inst
    }
}
