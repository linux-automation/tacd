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

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Channels(Vec<Channel>);

#[derive(Deserialize)]
pub struct ChannelFile {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub url: String,
    pub polling_interval: Option<String>,
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
            bundle: None,
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

        for dir_entry in dir_entries {
            let channel = Channel::from_file(&dir_entry.path())?;

            if channels.iter().any(|ch| ch.name == channel.name) {
                bail!("Encountered duplicate channel name \"{}\"", channel.name);
            }

            channels.push(channel);
        }

        Ok(Self(channels))
    }

    pub fn into_vec(self) -> Vec<Channel> {
        self.0
    }
}
