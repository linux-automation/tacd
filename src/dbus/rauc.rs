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

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_std::channel::Receiver;
use async_std::stream::StreamExt;
use async_std::sync::Arc;
use async_std::task::{sleep, spawn, JoinHandle};
use log::warn;
use serde::{Deserialize, Serialize};

use super::Connection;
use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

mod update_channels;
pub use update_channels::Channel;

#[cfg(feature = "demo_mode")]
mod demo_mode;

#[cfg(not(feature = "demo_mode"))]
mod installer;

#[cfg(not(feature = "demo_mode"))]
use installer::InstallerProxy;

#[cfg(feature = "demo_mode")]
mod imports {
    pub(super) struct InstallerProxy<'a> {
        _dummy: &'a (),
    }

    impl<'a> InstallerProxy<'a> {
        pub async fn new<C>(_conn: C) -> Option<InstallerProxy<'a>> {
            Some(Self { _dummy: &() })
        }

        pub async fn info(&self, _url: &str) -> anyhow::Result<(String, String)> {
            let compatible = "LXA TAC".to_string();
            let version = "4.0-0-20230428214619".to_string();

            Ok((compatible, version))
        }
    }

    pub(super) const CHANNELS_DIR: &str = "demo_files/usr/share/tacd/update_channels";
}

#[cfg(not(feature = "demo_mode"))]
mod imports {
    pub(super) use anyhow::bail;
    pub(super) use log::error;

    pub(super) const CHANNELS_DIR: &str = "/usr/share/tacd/update_channels";
}

const RELOAD_RATE_LIMIT: Duration = Duration::from_secs(10 * 60);

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

type SlotStatus = HashMap<String, HashMap<String, String>>;

pub struct Rauc {
    pub operation: Arc<Topic<String>>,
    pub progress: Arc<Topic<Progress>>,
    pub slot_status: Arc<Topic<Arc<SlotStatus>>>,
    pub primary: Arc<Topic<String>>,
    pub last_error: Arc<Topic<String>>,
    pub install: Arc<Topic<String>>,
    pub channels: Arc<Topic<Vec<Channel>>>,
    pub reload: Arc<Topic<bool>>,
    pub should_reboot: Arc<Topic<bool>>,
    pub enable_polling: Arc<Topic<bool>>,
}

fn compare_versions(v1: &str, v2: &str) -> Option<Ordering> {
    // Version strings look something like this: "4.0-0-20230428214619"
    // Use string sorting on the date part to determine which bundle is newer.
    let date_1 = v1.rsplit_once('-').map(|(_, d)| d);
    let date_2 = v2.rsplit_once('-').map(|(_, d)| d);

    // Return Sone if either version could not be split or a Some with the
    // ordering between the dates.
    date_1.zip(date_2).map(|(d1, d2)| d1.cmp(d2))
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

async fn channel_polling_task(
    conn: Arc<Connection>,
    enable_polling: Arc<Topic<bool>>,
    channels: Arc<Topic<Vec<Channel>>>,
    slot_status: Arc<Topic<Arc<SlotStatus>>>,
    name: String,
) {
    let proxy = InstallerProxy::new(&conn).await.unwrap();

    while let Some(mut channel) = channels
        .try_get()
        .and_then(|chs| chs.into_iter().find(|ch| ch.name == name))
    {
        // Make sure update polling is enabled before doing anything,
        // as contacting the update server requires user consent.
        enable_polling.wait_for(true).await;

        let polling_interval = channel.polling_interval;
        let slot_status = slot_status.try_get();

        if let Err(e) = channel.poll(&proxy, slot_status.as_deref()).await {
            warn!(
                "Failed to fetch update for update channel \"{}\": {}",
                channel.name, e
            );
        }

        channels.modify(|chs| {
            let mut chs = chs?;
            let channel_prev = chs.iter_mut().find(|ch| ch.name == name)?;

            // Check if the bundle we polled is the same as before and we don't need
            // to send a message to the subscribers.
            if *channel_prev == channel {
                return None;
            }

            // Update the channel description with the newly polled bundle info
            *channel_prev = channel;

            Some(chs)
        });

        match polling_interval {
            Some(pi) => sleep(pi).await,
            None => break,
        }
    }
}

async fn channel_list_update_task(
    conn: Arc<Connection>,
    mut reload_stream: Receiver<bool>,
    enable_polling: Arc<Topic<bool>>,
    channels: Arc<Topic<Vec<Channel>>>,
    slot_status: Arc<Topic<Arc<SlotStatus>>>,
) -> Result<()> {
    let mut previous: Option<Instant> = None;
    let mut polling_tasks: Vec<JoinHandle<_>> = Vec::new();

    while let Some(reload) = reload_stream.next().await {
        if !reload {
            continue;
        }

        // Polling for updates is a somewhat expensive operation.
        // Make sure it can not be abused to DOS the tacd.
        if previous
            .map(|p| p.elapsed() < RELOAD_RATE_LIMIT)
            .unwrap_or(false)
        {
            continue;
        }

        // Read the list of available update channels
        let new_channels = match Channel::from_directory(CHANNELS_DIR) {
            Ok(chs) => chs,
            Err(e) => {
                warn!("Failed to get list of update channels: {e}");
                continue;
            }
        };

        // Stop the currently running polling tasks
        for task in polling_tasks.drain(..) {
            task.cancel().await;
        }

        let names: Vec<String> = new_channels.iter().map(|c| c.name.clone()).collect();

        channels.set(new_channels);

        // Spawn new polling tasks. They will poll once immediately.
        for name in names.into_iter() {
            let polling_task = spawn(channel_polling_task(
                conn.clone(),
                enable_polling.clone(),
                channels.clone(),
                slot_status.clone(),
                name,
            ));

            polling_tasks.push(polling_task);
        }

        previous = Some(Instant::now());
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
            install: bb.topic_wo("/v1/tac/update/install", Some("".to_string())),
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
    ) -> Self {
        let inst = Self::setup_topics(bb);

        inst.operation.set("idle".to_string());
        inst.slot_status.set(Arc::new(demo_mode::slot_status()));
        inst.last_error.set("".to_string());

        // Reload the channel list on request
        let (reload_stream, _) = inst.reload.clone().subscribe_unbounded();
        wtb.spawn_task(
            "rauc-channel-list-update",
            channel_list_update_task(
                Arc::new(Connection),
                reload_stream,
                inst.enable_polling.clone(),
                inst.channels.clone(),
                inst.slot_status.clone(),
            ),
        );

        inst
    }

    #[cfg(not(feature = "demo_mode"))]
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        conn: &Arc<Connection>,
    ) -> Self {
        let inst = Self::setup_topics(bb);

        let conn_task = conn.clone();
        let operation = inst.operation.clone();
        let slot_status = inst.slot_status.clone();
        let primary = inst.primary.clone();
        let channels = inst.channels.clone();
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

                    // Update the `newer_than_installed` field for the upstream bundles inside
                    // of the update channels.
                    channels.modify(|prev| {
                        let prev = prev?;

                        let mut new = prev.clone();

                        for ch in new.iter_mut() {
                            if let Some(bundle) = ch.bundle.as_mut() {
                                bundle.update_install(&slots);
                            }
                        }

                        // Only send out messages if anything changed
                        (new != prev).then_some(new)
                    });

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
        });

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
        });

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
        });

        let conn_task = conn.clone();
        let (mut install_stream, _) = inst.install.clone().subscribe_unbounded();

        // Forward the "install" topic from the broker framework to RAUC
        wtb.spawn_task("rauc-forward-install", async move {
            let proxy = InstallerProxy::new(&conn_task).await.unwrap();

            while let Some(url) = install_stream.next().await {
                // Poor-mans validation. It feels wrong to let someone point to any
                // file on the TAC from the web interface.
                if url.starts_with("http://") || url.starts_with("https://") {
                    if let Err(e) = proxy.install(&url).await {
                        error!("Failed to install bundle: {}", e);
                    }
                }
            }

            Ok(())
        });

        // Reload the channel list on request
        let (reload_stream, _) = inst.reload.clone().subscribe_unbounded();
        wtb.spawn_task(
            "rauc-channel-list-update",
            channel_list_update_task(
                conn.clone(),
                reload_stream,
                inst.enable_polling.clone(),
                inst.channels.clone(),
                inst.slot_status.clone(),
            ),
        );

        inst
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{would_reboot_into_other_slot, SlotStatus};

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
                    panic!("Slot status {:?} with primary {:?} returned Ok({})) but should have errored", slots, primary, res);
                }
            }
        }
    }
}
