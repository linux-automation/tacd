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
// with this library; if not, see <https://www.gnu.org/licenses/>.

#[cfg(not(feature = "demo_mode"))]
use std::convert::TryFrom;
use std::fs::{DirEntry, read_dir, read_to_string};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

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
    pub manifest_hash: String,
    pub effective_url: String,
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
    pub primary: bool,
    pub bundle: Option<UpstreamBundle>,
    pub force_polling: Option<bool>,
    pub force_auto_install: Option<bool>,
    pub candidate_criteria: Option<String>,
    pub install_criteria: Option<String>,
    pub reboot_criteria: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Channels(Vec<Channel>);

#[derive(Deserialize)]
pub struct ChannelFile {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub url: String,
    pub polling_interval: Option<String>,
    pub force_polling: Option<bool>,
    pub force_auto_install: Option<bool>,
    pub candidate_criteria: Option<String>,
    pub install_criteria: Option<String>,
    pub reboot_criteria: Option<String>,
}

#[cfg(not(feature = "demo_mode"))]
fn zvariant_walk_nested_dicts<'a, T>(map: &'a zvariant::Dict, path: &'a [&'a str]) -> Result<&'a T>
where
    &'a T: TryFrom<&'a zvariant::Value<'a>>,
    <&'a T as TryFrom<&'a zvariant::Value<'a>>>::Error: Into<zvariant::Error>,
{
    let (key, rem) = path
        .split_first()
        .ok_or_else(|| anyhow!("Got an empty path to walk"))?;

    let value: &zvariant::Value = map
        .get(key)?
        .ok_or_else(|| anyhow!("Could not find key \"{key}\" in dict"))?;

    if rem.is_empty() {
        value.downcast_ref().map_err(|e| {
            let type_name = std::any::type_name::<T>();
            anyhow!("Failed to convert value in dictionary for key \"{key}\" to {type_name}: {e}")
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
            yaml_serde::from_str(&content)?
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
            primary: false,
            bundle: None,
            force_polling: channel_file.force_polling,
            force_auto_install: channel_file.force_auto_install,
            candidate_criteria: channel_file.candidate_criteria,
            install_criteria: channel_file.install_criteria,
            reboot_criteria: channel_file.reboot_criteria,
        };

        ch.update_enabled();

        Ok(ch)
    }

    fn update_enabled(&mut self) {
        // Which channels are enabled is decided based on which RAUC certificates are enabled.
        let cert_file = self.name.clone() + ".cert.pem";
        let cert_path = Path::new(ENABLE_DIR).join(cert_file);

        self.enabled = cert_path.exists();
    }
}

impl Channels {
    pub(super) fn from_directory(dir: &str) -> Result<Self> {
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

        let mut channels: Vec<Channel> = Vec::new();

        let mut have_primary = false;

        for dir_entry in dir_entries {
            let mut channel = Channel::from_file(&dir_entry.path())?;

            if channels.iter().any(|ch| ch.name == channel.name) {
                bail!("Encountered duplicate channel name \"{}\"", channel.name);
            }

            // There can only be one primary channel.
            // If multiple channels are enabled the primary one is the one with
            // the highest precedence.
            channel.primary = channel.enabled && !have_primary;
            have_primary |= channel.primary;

            channels.push(channel);
        }

        Ok(Self(channels))
    }

    pub fn into_vec(self) -> Vec<Channel> {
        self.0
    }

    pub(super) fn primary(&self) -> Option<&Channel> {
        self.0.iter().find(|ch| ch.primary)
    }

    #[cfg(not(feature = "demo_mode"))]
    fn primary_mut(&mut self) -> Option<&mut Channel> {
        self.0.iter_mut().find(|ch| ch.primary)
    }

    #[cfg(not(feature = "demo_mode"))]
    pub(super) fn update_from_poll_status(&mut self, poll_status: zvariant::Dict) -> Result<bool> {
        let compatible: &zvariant::Str =
            zvariant_walk_nested_dicts(&poll_status, &["manifest", "update", "compatible"])?;
        let version: &zvariant::Str =
            zvariant_walk_nested_dicts(&poll_status, &["manifest", "update", "version"])?;
        let manifest_hash: &zvariant::Str =
            zvariant_walk_nested_dicts(&poll_status, &["manifest", "manifest-hash"])?;
        let effective_url: &zvariant::Str =
            zvariant_walk_nested_dicts(&poll_status, &["bundle", "effective-url"])?;
        let newer_than_installed: &bool =
            zvariant_walk_nested_dicts(&poll_status, &["update-available"])?;

        if let Some(pb) = self.0.iter().find_map(|ch| ch.bundle.as_ref())
            && compatible == pb.compatible.as_str()
            && version == pb.version.as_str()
            && manifest_hash == pb.manifest_hash.as_str()
            && effective_url == pb.effective_url.as_str()
            && *newer_than_installed == pb.newer_than_installed
        {
            return Ok(false);
        }

        self.0.iter_mut().for_each(|ch| ch.bundle = None);

        if let Some(primary) = self.primary_mut() {
            primary.bundle = Some(UpstreamBundle {
                compatible: compatible.as_str().into(),
                version: version.as_str().into(),
                manifest_hash: manifest_hash.as_str().into(),
                effective_url: effective_url.as_str().into(),
                newer_than_installed: *newer_than_installed,
            });
        }

        Ok(true)
    }
}
