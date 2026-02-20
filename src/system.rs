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

use std::ffi::OsStr;

use anyhow::{anyhow, bail, Result};
use async_std::sync::Arc;
use nix::sys::utsname::uname;
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

#[cfg(feature = "demo_mode")]
mod read_dt_props {
    use anyhow::{anyhow, Result};

    const DEMO_DATA_STR: &[(&str, &str)] = &[
        (
            "compatible",
            "lxa,stm32mp153c-tac-gen3\0oct,stm32mp15xx-osd32\0st,stm32mp153",
        ),
        ("chosen/barebox-version", "barebox-2022.11.0-20221121-1"),
        (
            "chosen/baseboard-factory-data/pcba-hardware-release",
            "lxatac-S01-R03-B02-C00",
        ),
        (
            "chosen/powerboard-factory-data/pcba-hardware-release",
            "lxatac-S05-R03-V01-C00",
        ),
        (
            "chosen/baseboard-factory-data/featureset",
            "base,tft,calibrated",
        ),
        (
            "chosen/powerboard-factory-data/featureset",
            "base,calibrated",
        ),
    ];

    const DEMO_DATA_NUM: &[(&str, u32)] = &[
        ("chosen/baseboard-factory-data/modification", 0),
        (
            "chosen/baseboard-factory-data/factory-timestamp",
            1678086417,
        ),
        ("chosen/powerboard-factory-data/modification", 0),
        (
            "chosen/powerboard-factory-data/factory-timestamp",
            1678086418,
        ),
    ];

    pub fn read_dt_property(path: &str) -> Result<String> {
        let (_, content) = DEMO_DATA_STR
            .iter()
            .find(|(p, _)| *p == path)
            .ok_or_else(|| anyhow!("could not find devicetree property {path}"))?;

        Ok(content.to_string())
    }

    pub fn read_dt_property_u32(path: &str) -> Result<u32> {
        let (_, content) = DEMO_DATA_NUM
            .iter()
            .find(|(p, _)| *p == path)
            .ok_or_else(|| anyhow!("could not find devicetree property {path}"))?;

        Ok(*content)
    }
}

#[cfg(not(feature = "demo_mode"))]
mod read_dt_props {
    use std::fs::read;
    use std::str::from_utf8;

    use anyhow::{anyhow, Result};

    const DT_BASE: &str = "/sys/firmware/devicetree/base/";

    pub fn read_dt_property(path: &str) -> Result<String> {
        let path = [DT_BASE, path].join("/");
        let bytes = read(&path)?;
        let stripped_bytes = bytes
            .strip_suffix(&[0])
            .ok_or_else(|| anyhow!("Devicetree property {path} did not contain a value"))?;
        let stripped = from_utf8(stripped_bytes)?;

        Ok(stripped.to_string())
    }

    pub fn read_dt_property_u32(path: &str) -> Result<u32> {
        let raw = read_dt_property(path)?;
        let value = raw.parse()?;

        Ok(value)
    }
}

use read_dt_props::{read_dt_property, read_dt_property_u32};

#[derive(Serialize, Deserialize)]
pub struct Uname {
    pub sysname: String,
    pub nodename: String,
    pub release: String,
    pub version: String,
    pub machine: String,
}

impl Uname {
    fn get() -> Result<Self> {
        let uts = uname()?;

        fn to_string(val: &OsStr, name: &str) -> Result<String> {
            let res = val
                .to_str()
                .ok_or_else(|| anyhow!("uname entry {name} can not be converted to utf-8"))?
                .to_string();

            Ok(res)
        }

        Ok(Self {
            sysname: to_string(uts.sysname(), "sysname")?,
            nodename: to_string(uts.nodename(), "nodename")?,
            release: to_string(uts.release(), "release")?,
            version: to_string(uts.version(), "version")?,
            machine: to_string(uts.machine(), "machine")?,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct Barebox {
    pub version: String,
    pub baseboard_release: String,
    pub powerboard_release: String,
    pub baseboard_timestamp: u32,
    pub powerboard_timestamp: u32,
    pub baseboard_featureset: Vec<String>,
    pub powerboard_featureset: Vec<String>,
}

impl Barebox {
    fn get() -> Result<Self> {
        // Get info from devicetree chosen
        Ok(Self {
            version: read_dt_property("chosen/barebox-version")?,
            baseboard_release: {
                let template =
                    read_dt_property("chosen/baseboard-factory-data/pcba-hardware-release")?;
                let changeset = read_dt_property_u32("chosen/baseboard-factory-data/modification")?;
                template.replace("-C??", &format!("-C{changeset:02}"))
            },
            powerboard_release: {
                let template =
                    read_dt_property("chosen/powerboard-factory-data/pcba-hardware-release")?;
                let changeset =
                    read_dt_property_u32("chosen/powerboard-factory-data/modification")?;

                template.replace("-C??", &format!("-C{changeset:02}"))
            },
            baseboard_timestamp: {
                read_dt_property_u32("chosen/baseboard-factory-data/factory-timestamp")?
            },
            powerboard_timestamp: {
                read_dt_property_u32("chosen/powerboard-factory-data/factory-timestamp")?
            },
            baseboard_featureset: {
                read_dt_property("chosen/baseboard-factory-data/featureset")?
                    .split(',')
                    .map(str::to_string)
                    .collect()
            },
            powerboard_featureset: {
                read_dt_property("chosen/powerboard-factory-data/featureset")?
                    .split(',')
                    .map(str::to_string)
                    .collect()
            },
        })
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum HardwareGeneration {
    Gen1,
    Gen2,
    Gen3,
}

impl HardwareGeneration {
    pub fn get() -> Result<Self> {
        let compatible = read_dt_property("compatible")?;

        // The compatible property consists of strings separated by NUL bytes.
        // We are interested in the first of these strings.
        let device = compatible.split('\0').next().unwrap_or("<empty>");

        match device {
            "lxa,stm32mp157c-tac-gen1" => Ok(Self::Gen1),
            "lxa,stm32mp157c-tac-gen2" => Ok(Self::Gen2),
            "lxa,stm32mp153c-tac-gen3" => Ok(Self::Gen3),
            generation => bail!("Running on unknown LXA TAC hardware generation \"{generation}\""),
        }
    }
}

pub struct System {
    #[allow(dead_code)]
    pub uname: Arc<Topic<Arc<Uname>>>,
    #[allow(dead_code)]
    pub barebox: Arc<Topic<Arc<Barebox>>>,
    #[allow(dead_code)]
    pub tacd_version: Arc<Topic<String>>,
    #[allow(dead_code)]
    pub hardware_generation: Arc<Topic<HardwareGeneration>>,
}

impl System {
    pub fn new(bb: &mut BrokerBuilder, hardware_generation: HardwareGeneration) -> Result<Self> {
        let version = env!("VERSION_STRING").to_string();

        let uname = Uname::get()?;
        let barebox = Barebox::get()?;

        Ok(Self {
            uname: bb.topic_ro("/v1/tac/info/uname", Some(Arc::new(uname))),
            barebox: bb.topic_ro("/v1/tac/info/bootloader", Some(Arc::new(barebox))),
            tacd_version: bb.topic_ro("/v1/tac/info/tacd/version", Some(version)),
            hardware_generation: bb.topic_ro(
                "/v1/tac/info/hardware_generation",
                Some(hardware_generation),
            ),
        })
    }
}
