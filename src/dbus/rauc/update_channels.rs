// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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
use std::fs::{read_dir, read_to_string, DirEntry};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use super::{compare_versions, InstallerProxy, SlotStatus};

#[cfg(feature = "demo_mode")]
const ENABLE_DIR: &str = "demo_files/etc/rauc/certificates-enabled";

#[cfg(not(feature = "demo_mode"))]
const ENABLE_DIR: &str = "/etc/rauc/certificates-enabled";

const ONE_MINUTE: Duration = Duration::from_secs(60);
const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct UpstreamBundle {
    pub compatible: String,
    pub version: String,
    pub newer_than_installed: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Channel {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub url: String,
    pub polling_interval: Option<Duration>,
    pub enabled: bool,
    pub bundle: Option<UpstreamBundle>,
}

#[derive(Deserialize)]
pub struct ChannelFile {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub url: String,
    pub polling_interval: Option<String>,
}

fn zvariant_walk_nested_dicts(map: &zvariant::Dict, path: &[&str]) -> Result<String> {
    let (&key, rem) = path
        .split_first()
        .ok_or_else(|| anyhow!("Got an empty path to walk"))?;

    let value: &zvariant::Value = map
        .get(&key)?
        .ok_or_else(|| anyhow!("Could not find key \"{key}\" in dict"))?;

    if rem.is_empty() {
        value.downcast_ref().map_err(|e| {
            anyhow!("Failed to convert value in dictionary for key \"{key}\" to a string: {e}")
        })
    } else {
        let value = value.downcast_ref().map_err(|e| {
            anyhow!("Failed to convert value in dictionary for key \"{key}\" to a dict: {e}")
        })?;

        zvariant_walk_nested_dicts(value, rem)
    }
}

impl Channel {
    fn from_file(path: &Path) -> Result<Self> {
        let file_name = || {
            path.file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("<no filename>")
        };

        let mut channel_file: ChannelFile = {
            let content = read_to_string(path)?;
            serde_yaml::from_str(&content)?
        };

        let polling_interval = match channel_file.polling_interval.take() {
            Some(mut pi) => {
                let multiplier = match pi.pop() {
                    Some('m') => ONE_MINUTE,
                    Some('h') => ONE_HOUR,
                    Some('d') => ONE_DAY,
                    _ => {
                        bail!(
                        "The polling_interval in \"{}\" does not have one of m, h or d as suffix",
                        file_name()
                    );
                    }
                };

                let value: u32 = pi.parse().map_err(|e| {
                    anyhow!(
                        "Failed to parse polling_interval in \"{}\": {}",
                        file_name(),
                        e
                    )
                })?;

                (value != 0).then_some(multiplier * value)
            }
            None => None,
        };

        let mut ch = Self {
            name: channel_file.name,
            display_name: channel_file.display_name,
            description: channel_file.description,
            url: channel_file.url.trim().to_string(),
            polling_interval,
            enabled: false,
            bundle: None,
        };

        ch.update_enabled();

        Ok(ch)
    }

    pub(super) fn from_directory(dir: &str) -> Result<Vec<Self>> {
        // Find all .yaml files in CHANNELS_DIR
        let mut dir_entries: Vec<DirEntry> = read_dir(dir)?
            .filter_map(|dir_entry| dir_entry.ok())
            .filter(|dir_entry| {
                dir_entry
                    .file_name()
                    .as_os_str()
                    .as_bytes()
                    .ends_with(b".yaml")
            })
            .collect();

        // And sort them alphabetically, so that 01_stable.yaml takes precedence over
        // 05_testing.yaml.
        dir_entries.sort_by_key(|dir_entry| dir_entry.file_name());

        let mut channels: Vec<Self> = Vec::new();

        for dir_entry in dir_entries {
            let channel = Self::from_file(&dir_entry.path())?;

            if channels.iter().any(|ch| ch.name == channel.name) {
                bail!("Encountered duplicate channel name \"{}\"", channel.name);
            }

            channels.push(channel);
        }

        Ok(channels)
    }

    fn update_enabled(&mut self) {
        // Which channels are enabled is decided based on which RAUC certificates are enabled.
        let cert_file = self.name.clone() + ".cert.pem";
        let cert_path = Path::new(ENABLE_DIR).join(cert_file);

        self.enabled = cert_path.exists();
    }

    /// Ask RAUC to determine the version of the bundle on the server
    pub(super) async fn poll(
        &mut self,
        proxy: &InstallerProxy<'_>,
        slot_status: Option<&SlotStatus>,
    ) -> Result<()> {
        self.update_enabled();

        self.bundle = None;

        if self.enabled {
            let args = HashMap::new();
            let bundle = proxy.inspect_bundle(&self.url, args).await?;
            let bundle: zvariant::Dict = bundle.into();

            let compatible =
                zvariant_walk_nested_dicts(&bundle, &["update", "compatible"])?.to_owned();
            let version = zvariant_walk_nested_dicts(&bundle, &["update", "version"])?.to_owned();

            self.bundle = Some(UpstreamBundle::new(compatible, version, slot_status));
        }

        Ok(())
    }
}

impl UpstreamBundle {
    fn new(compatible: String, version: String, slot_status: Option<&SlotStatus>) -> Self {
        let mut ub = Self {
            compatible,
            version,
            newer_than_installed: false,
        };

        if let Some(slot_status) = slot_status {
            ub.update_install(slot_status);
        }

        ub
    }

    pub(super) fn update_install(&mut self, slot_status: &SlotStatus) {
        let slot_0_is_older = slot_status
            .get("rootfs_0")
            .filter(|r| r.get("boot_status").map_or(false, |b| b == "good"))
            .and_then(|r| r.get("bundle_version"))
            .and_then(|v| compare_versions(&self.version, v).map(|c| c.is_gt()))
            .unwrap_or(true);

        let slot_1_is_older = slot_status
            .get("rootfs_1")
            .filter(|r| r.get("boot_status").map_or(false, |b| b == "good"))
            .and_then(|r| r.get("bundle_version"))
            .and_then(|v| compare_versions(&self.version, v).map(|c| c.is_gt()))
            .unwrap_or(true);

        self.newer_than_installed = slot_0_is_older && slot_1_is_older;
    }
}
