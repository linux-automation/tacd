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

use std::collections::HashMap;

use anyhow::Result;
use async_std::channel::Receiver;
use async_std::stream::StreamExt;
use async_std::sync::Arc;
use log::warn;
use serde::{Deserialize, Serialize};

use super::Connection;
use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

mod update_channels;
pub use update_channels::{Channel, Channels};

#[cfg(feature = "demo_mode")]
mod demo_mode;

#[cfg(not(feature = "demo_mode"))]
mod installer;

#[cfg(not(feature = "demo_mode"))]
use installer::InstallerProxy;

#[cfg(not(feature = "demo_mode"))]
mod poller;

#[cfg(feature = "demo_mode")]
mod imports {
    pub(super) const CHANNELS_DIR: &str = "demo_files/usr/share/tacd/update_channels";
}

#[cfg(not(feature = "demo_mode"))]
mod imports {
    pub(super) use anyhow::bail;
    pub(super) use log::error;

    pub(super) const CHANNELS_DIR: &str = "/usr/share/tacd/update_channels";
}

use imports::*;

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

#[derive(Serialize, Deserialize, Clone)]
#[serde(from = "UpdateRequestDe")]
pub struct UpdateRequest {
    pub manifest_hash: Option<String>,
    pub url: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UpdateRequestDe {
    UrlAndHash {
        manifest_hash: Option<String>,
        url: Option<String>,
    },
    UrlOnly(String),
}

impl From<UpdateRequestDe> for UpdateRequest {
    fn from(de: UpdateRequestDe) -> Self {
        // Provide API backward compatibility by allowing either just a String
        // as argument or a map with url and manifest hash inside.
        match de {
            UpdateRequestDe::UrlAndHash { manifest_hash, url } => Self { manifest_hash, url },
            UpdateRequestDe::UrlOnly(url) => Self {
                manifest_hash: None,
                url: Some(url),
            },
        }
    }
}

type SlotStatus = HashMap<String, HashMap<String, String>>;

pub struct Rauc {
    pub operation: Arc<Topic<String>>,
    pub progress: Arc<Topic<Progress>>,
    pub slot_status: Arc<Topic<Arc<SlotStatus>>>,
    #[cfg_attr(feature = "demo_mode", allow(dead_code))]
    pub primary: Arc<Topic<String>>,
    pub last_error: Arc<Topic<String>>,
    pub install: Arc<Topic<UpdateRequest>>,
    pub channels: Arc<Topic<Channels>>,
    pub reload: Arc<Topic<bool>>,
    pub should_reboot: Arc<Topic<bool>>,
    #[allow(dead_code)]
    pub enable_polling: Arc<Topic<bool>>,
}

#[cfg(not(feature = "demo_mode"))]
fn would_reboot_into_other_slot(slot_status: &SlotStatus, primary: Option<String>) -> Result<bool> {
    let rootfs_0 = slot_status.get("rootfs_0");
    let rootfs_1 = slot_status.get("rootfs_1");

    let (rootfs_0_is_primary, rootfs_1_is_primary) = primary
        .map(|p| (p == "rootfs_0", p == "rootfs_1"))
        .unwrap_or((false, false));

    let rootfs_0_booted = rootfs_0.and_then(|r| r.get("state")).map(|s| s == "booted");
    let rootfs_1_booted = rootfs_1.and_then(|r| r.get("state")).map(|s| s == "booted");

    let ((booted_slot, booted_is_primary), (other_slot, other_is_primary)) =
        match (rootfs_0_booted, rootfs_1_booted) {
            (Some(true), Some(true)) => {
                bail!("Two booted RAUC slots at the same time");
            }
            (Some(true), _) => (
                (rootfs_0, rootfs_0_is_primary),
                (rootfs_1, rootfs_1_is_primary),
            ),
            (_, Some(true)) => (
                (rootfs_1, rootfs_1_is_primary),
                (rootfs_0, rootfs_0_is_primary),
            ),
            _ => {
                bail!("No booted RAUC slot");
            }
        };

    let booted_good = booted_slot
        .and_then(|r| r.get("boot_status"))
        .map(|s| s == "good");
    let other_good = other_slot
        .and_then(|r| r.get("boot_status"))
        .map(|s| s == "good");

    let booted_ok = booted_slot.and_then(|r| r.get("status")).map(|s| s == "ok");
    let other_ok = other_slot.and_then(|r| r.get("status")).map(|s| s == "ok");

    let booted_viable = booted_good.unwrap_or(false) && booted_ok.unwrap_or(false);
    let other_viable = other_good.unwrap_or(false) && other_ok.unwrap_or(false);

    match (
        booted_viable,
        other_viable,
        booted_is_primary,
        other_is_primary,
    ) {
        (true, false, _, _) => Ok(false),
        (false, true, _, _) => Ok(true),
        (true, true, true, false) => Ok(false),
        (true, true, false, true) => Ok(true),
        (false, false, _, _) => bail!("No bootable slot present"),
        (_, _, false, false) => bail!("No primary slot present"),
        (true, true, true, true) => bail!("Two primary slots present"),
    }
}

async fn channel_list_update_task(
    mut reload_stream: Receiver<bool>,
    channels: Arc<Topic<Channels>>,
) -> Result<()> {
    while let Some(reload) = reload_stream.next().await {
        if !reload {
            continue;
        }

        // Read the list of available update channels
        let new_channels = match Channels::from_directory(CHANNELS_DIR) {
            Ok(chs) => chs,
            Err(e) => {
                warn!("Failed to get list of update channels: {e}");
                continue;
            }
        };

        channels.set(new_channels);
    }

    Ok(())
}

impl Rauc {
    fn setup_topics(bb: &mut BrokerBuilder) -> Self {
        Self {
            operation: bb.topic_ro("/v1/tac/update/operation", None),
            progress: bb.topic_ro("/v1/tac/update/progress", None),
            slot_status: bb.topic_ro("/v1/tac/update/slots", None),
            primary: bb.topic_ro("/v1/tac/update/primary", None),
            last_error: bb.topic_ro("/v1/tac/update/last_error", None),
            install: bb.topic_wo("/v1/tac/update/install", None),
            channels: bb.topic_ro("/v1/tac/update/channels", None),
            reload: bb.topic_wo("/v1/tac/update/channels/reload", Some(true)),
            should_reboot: bb.topic_ro("/v1/tac/update/should_reboot", Some(false)),
            enable_polling: bb.topic(
                "/v1/tac/update/enable_polling",
                true,
                true,
                true,
                Some(false),
                1,
            ),
        }
    }

    #[cfg(feature = "demo_mode")]
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        _conn: &Arc<Connection>,
    ) -> Result<Self> {
        let inst = Self::setup_topics(bb);

        inst.operation.set("idle".to_string());
        inst.slot_status.set(Arc::new(demo_mode::slot_status()));
        inst.last_error.set("".to_string());

        // Reload the channel list on request
        let (reload_stream, _) = inst.reload.clone().subscribe_unbounded();
        wtb.spawn_task(
            "rauc-channel-list-update",
            channel_list_update_task(reload_stream, inst.channels.clone()),
        )?;

        Ok(inst)
    }

    #[cfg(not(feature = "demo_mode"))]
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        conn: &Arc<Connection>,
    ) -> Result<Self> {
        let inst = Self::setup_topics(bb);

        let conn_task = conn.clone();
        let operation = inst.operation.clone();
        let slot_status = inst.slot_status.clone();
        let primary = inst.primary.clone();
        let should_reboot = inst.should_reboot.clone();

        wtb.spawn_task("rauc-slot-status-update", async move {
            let proxy = InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_operation_changed().await;

            if let Ok(v) = proxy.operation().await {
                operation.set(v);
            }

            loop {
                // Update which slot is considered the primary whenever the current
                // operation changes.
                // (The one that should be booted next _if it is bootable_)
                let new_primary = proxy.get_primary().await.ok().map(|p| p.replace('.', "_"));

                if let Some(p) = new_primary.clone() {
                    primary.set_if_changed(p);
                }

                // Refresh the slot status whenever the current operation changes
                // This is mostly relevant for "installing" -> "idle" transitions
                // but it can't hurt to do it on any transition.
                if let Ok(slots) = proxy.get_slot_status().await {
                    let slots = slots
                        .into_iter()
                        .map(|(slot_name, slot_info)| {
                            let mut info: HashMap<String, String> = slot_info
                                .into_iter()
                                .map(|(k, v)| {
                                    // Convert integers to strings as raw zvariant values are
                                    // unusable when json serialized and I can not be bothered
                                    // to fiddle around with an enum that wraps strings and integers
                                    // or something like that
                                    let ss = v.downcast_ref::<String>();
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

                    // Provide a simple yes/no "should reboot into other slot?" information
                    // based on the bundle versions in the booted slot and the other slot.
                    match would_reboot_into_other_slot(&slots, new_primary) {
                        Ok(b) => should_reboot.set_if_changed(b),
                        Err(e) => warn!("Could not determine if TAC should be rebooted: {e}"),
                    }

                    // In the RAUC API the slot status is a list of (name, info) tuples.
                    // It is once again easier in typescript to represent it as a dict with
                    // the names as keys, so that is what's exposed here.
                    slot_status.set(Arc::new(slots));
                }

                // Wait for the current operation to change
                if let Some(v) = stream.next().await {
                    if let Ok(v) = v.get().await {
                        operation.set(v);
                    }
                } else {
                    break Ok(());
                }
            }
        })?;

        let conn_task = conn.clone();
        let progress = inst.progress.clone();

        // Forward the "progress" property to the broker framework
        wtb.spawn_task("rauc-progress-update", async move {
            let proxy = InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_progress_changed().await;

            if let Ok(p) = proxy.progress().await {
                progress.set(p.into());
            }

            while let Some(v) = stream.next().await {
                if let Ok(p) = v.get().await {
                    progress.set(p.into());
                }
            }

            Ok(())
        })?;

        let conn_task = conn.clone();
        let last_error = inst.last_error.clone();

        // Forward the "last_error" property to the broker framework
        wtb.spawn_task("rauc-forward-error", async move {
            let proxy = InstallerProxy::new(&conn_task).await.unwrap();

            let mut stream = proxy.receive_last_error_changed().await;

            if let Ok(e) = proxy.last_error().await {
                last_error.set(e);
            }

            while let Some(v) = stream.next().await {
                if let Ok(e) = v.get().await {
                    last_error.set(e);
                }
            }

            Ok(())
        })?;

        let conn_task = conn.clone();
        let channels = inst.channels.clone();
        let (mut install_stream, _) = inst.install.clone().subscribe_unbounded();

        // Forward the "install" topic from the broker framework to RAUC
        wtb.spawn_task("rauc-forward-install", async move {
            let proxy = InstallerProxy::new(&conn_task).await.unwrap();

            while let Some(update_request) = install_stream.next().await {
                let channels = match channels.try_get() {
                    Some(chs) => chs,
                    None => {
                        warn!("Got install request with no channels available yet");
                        continue;
                    }
                };

                let primary = match channels.primary() {
                    Some(primary) => primary,
                    None => {
                        warn!("Got install request with no primary channel configured");
                        continue;
                    }
                };

                let url = match &update_request.url {
                    None => &primary.url,
                    Some(url) if url == &primary.url => &primary.url,
                    Some(_) => {
                        warn!("Got install request with URL not matching primary channel URL");
                        continue;
                    }
                };

                let manifest_hash: Option<zbus::zvariant::Value> =
                    update_request.manifest_hash.map(|mh| mh.into());

                let mut args = HashMap::new();

                if let Some(manifest_hash) = &manifest_hash {
                    args.insert("require-manifest-hash", manifest_hash);
                }

                if let Err(e) = proxy.install_bundle(url, args).await {
                    error!("Failed to install bundle: {}", e);
                }
            }

            Ok(())
        })?;

        // Reload the channel list on request
        let (reload_stream, _) = inst.reload.clone().subscribe_unbounded();
        wtb.spawn_task(
            "rauc-channel-list-update",
            channel_list_update_task(reload_stream, inst.channels.clone()),
        )?;

        Ok(inst)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{SlotStatus, would_reboot_into_other_slot};

    #[test]
    fn reboot_notifications() {
        let bootable = HashMap::from([
            ("boot_status".to_string(), "good".to_string()),
            ("status".to_string(), "ok".to_string()),
        ]);

        let not_bootable = HashMap::from([
            ("boot_status".to_string(), "bad".to_string()),
            ("status".to_string(), "ok".to_string()),
        ]);

        let cases = [
            (bootable.clone(), bootable.clone(), 0, 1, Ok(true)),
            (bootable.clone(), bootable.clone(), 1, 0, Ok(true)),
            (bootable.clone(), bootable.clone(), 0, 0, Ok(false)),
            (bootable.clone(), bootable.clone(), 1, 1, Ok(false)),
            (not_bootable.clone(), bootable.clone(), 1, 0, Ok(false)),
            (bootable.clone(), not_bootable.clone(), 0, 1, Ok(false)),
            (not_bootable.clone(), bootable.clone(), 0, 0, Ok(true)),
            (bootable.clone(), not_bootable.clone(), 1, 1, Ok(true)),
            (not_bootable.clone(), not_bootable.clone(), 0, 1, Err(())),
            (bootable.clone(), bootable.clone(), 2, 0, Err(())),
            (bootable.clone(), bootable.clone(), 0, 2, Err(())),
        ];

        for (mut rootfs_0, mut rootfs_1, booted, primary, expected) in cases {
            let slots = {
                rootfs_0.insert(
                    "state".to_string(),
                    if booted == 0 {
                        "booted".to_string()
                    } else {
                        "inactive".to_string()
                    },
                );

                rootfs_1.insert(
                    "state".to_string(),
                    if booted == 1 {
                        "booted".to_string()
                    } else {
                        "inactive".to_string()
                    },
                );

                SlotStatus::from([
                    ("rootfs_0".to_string(), rootfs_0),
                    ("rootfs_1".to_string(), rootfs_1),
                ])
            };

            let primary = Some(format!("rootfs_{primary}"));

            let res = would_reboot_into_other_slot(&slots, primary.clone());

            match (res, expected) {
                (Ok(true), Ok(true)) | (Ok(false), Ok(false)) | (Err(_), Err(_)) => {}
                (Ok(r), Ok(e)) => {
                    eprintln!(
                        "Slot status {slots:?} with primary {primary:?} yielded wrong result"
                    );
                    assert_eq!(r, e);
                }
                (Err(e), Ok(_)) => {
                    eprintln!(
                        "Slot status {slots:?} with primary {primary:?} returned unexpected error"
                    );
                    panic!("{:?}", e);
                }
                (Ok(res), Err(_)) => {
                    panic!(
                        "Slot status {:?} with primary {:?} returned Ok({})) but should have errored",
                        slots, primary, res
                    );
                }
            }
        }
    }
}
