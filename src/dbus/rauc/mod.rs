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

use std::collections::HashMap;

use async_std::sync::Arc;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "demo_mode"))]
use async_std::prelude::*;

#[cfg(not(feature = "demo_mode"))]
use async_std::task::spawn;

use super::Connection;
use crate::broker::{BrokerBuilder, Topic};

#[cfg(feature = "demo_mode")]
mod demo_mode;

#[cfg(not(feature = "demo_mode"))]
mod installer;

#[derive(Serialize, Deserialize, Clone)]
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
    pub slot_status: Arc<Topic<Arc<SlotStatus>>>,
    pub last_error: Arc<Topic<String>>,
    pub install: Arc<Topic<String>>,
}

impl Rauc {
    fn setup_topics(bb: &mut BrokerBuilder) -> Self {
        Self {
            operation: bb.topic_ro("/v1/tac/update/operation", None),
            progress: bb.topic_ro("/v1/tac/update/progress", None),
            slot_status: bb.topic_ro("/v1/tac/update/slots", None),
            last_error: bb.topic_ro("/v1/tac/update/last_error", None),
            install: bb.topic_wo("/v1/tac/update/install", Some("".to_string())),
        }
    }

    #[cfg(feature = "demo_mode")]
    pub async fn new(bb: &mut BrokerBuilder, _conn: &Arc<Connection>) -> Self {
        let inst = Self::setup_topics(bb);

        inst.operation.set("idle".to_string()).await;
        inst.slot_status
            .set(Arc::new(demo_mode::slot_status()))
            .await;
        inst.last_error.set("".to_string()).await;

        inst
    }

    #[cfg(not(feature = "demo_mode"))]
    pub async fn new(bb: &mut BrokerBuilder, conn: &Arc<Connection>) -> Self {
        let inst = Self::setup_topics(bb);

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
                // This is mostly relevant for "installing" -> "idle" transitions
                // but it can't hurt to do it on any transition.
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

                                    // Some of the field names make defining a "RaucSlot" type
                                    // in Typescript difficult. Not matching the names defined
                                    // in RAUC's API is also not great, but the lesser evil in
                                    // this case.
                                    let k = k
                                        .replace("type", "fs_type")
                                        .replace("class", "slot_class")
                                        .replace(['.', '-'], "_");

                                    (k, ss.or(s32).or(s64).unwrap_or_default())
                                })
                                .collect();

                            // Include the (unmangled) slot name as a field in the slot
                            // dict, once again to make life in the Web Interface easier.
                            info.insert("name".to_string(), slot_name.clone());

                            // Remove "." from the dictionary key to make defining a typescript
                            // type easier ("rootfs.0" -> "rootfs_0").
                            (slot_name.replace('.', "_"), info)
                        })
                        .collect();

                    // In the RAUC API the slot status is a list of (name, info) tuples.
                    // It is once again easier in typescript to represent it as a dict with
                    // the names as keys, so that is what's exposed here.
                    slot_status.set(Arc::new(slots)).await;
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

        // Forward the "progress" property to the broker framework
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

        // Forward the "last_error" property to the broker framework
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

        // Forward the "install" topic from the broker framework to RAUC
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
