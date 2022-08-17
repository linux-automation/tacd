use std::collections::HashMap;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use serde::{Deserialize, Serialize};
use zbus::Connection;
use zvariant::OwnedValue;

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

type SlotStatus = Vec<(String, HashMap<String, OwnedValue>)>;

pub struct Rauc {
    pub operation: Arc<Topic<String>>,
    pub progress: Arc<Topic<Progress>>,
    pub slot_status: Arc<Topic<SlotStatus>>,
}

impl Rauc {
    async fn update_slot_status(&self, conn: Arc<Connection>) {
        let proxy = installer::InstallerProxy::new(&conn).await.unwrap();

        if let Ok(s) = proxy.get_slot_status().await {
            self.slot_status.set(s).await;
        }
    }

    pub async fn new(bb: &mut BrokerBuilder, conn: Arc<Connection>) -> Self {
        let inst = Self {
            operation: bb.topic_ro("/v1/tac/update/operation"),
            progress: bb.topic_ro("/v1/tac/update/progress"),
            slot_status: bb.topic_ro("/v1/tac/update/slots"),
        };

        let conn_task = conn.clone();
        let operation = inst.operation.clone();

        spawn(async move {
            let proxy = installer::InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_operation_changed().await;
            while let Some(v) = stream.next().await {
                if let Ok(v) = v.get().await {
                    operation.set(v).await;
                }
            }
        });

        let conn_task = conn.clone();
        let progress = inst.progress.clone();

        spawn(async move {
            let proxy = installer::InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_progress_changed().await;
            while let Some(v) = stream.next().await {
                if let Ok(p) = v.get().await {
                    progress.set(p.into()).await;
                }
            }
        });

        inst.update_slot_status(conn).await;

        inst
    }
}
