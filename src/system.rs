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

use async_std::sync::Arc;
use nix::sys::utsname::uname;
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

#[cfg(any(test, feature = "stub_out_barebox"))]
mod read_dt_props {
    pub fn read_dt_property(_: &str) -> String {
        "stub".to_string()
    }

    pub fn read_dt_property_u32(_: &str) -> u32 {
        0
    }
}

#[cfg(not(any(test, feature = "stub_out_barebox")))]
mod read_dt_props {
    use std::fs::read;
    use std::str::from_utf8;

    const DT_CHOSEN: &str = "/sys/firmware/devicetree/base/chosen/";

    pub fn read_dt_property(path: &str) -> String {
        let bytes = read([DT_CHOSEN, path].join("/")).unwrap();
        from_utf8(bytes.strip_suffix(&[0]).unwrap())
            .unwrap()
            .to_string()
    }

    pub fn read_dt_property_u32(path: &str) -> u32 {
        read_dt_property(path).parse().unwrap()
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
    fn get() -> Self {
        let uts = uname().unwrap();

        Self {
            sysname: uts.sysname().to_str().unwrap().to_string(),
            nodename: uts.nodename().to_str().unwrap().to_string(),
            release: uts.release().to_str().unwrap().to_string(),
            version: uts.version().to_str().unwrap().to_string(),
            machine: uts.machine().to_str().unwrap().to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Barebox {
    pub version: String,
    pub baseboard_release: String,
    pub powerboard_release: String,
    pub baseboard_timestamp: u32,
    pub powerboard_timestamp: u32,
}

impl Barebox {
    fn get() -> Self {
        // Get info from devicetree chosen
        Self {
            version: read_dt_property("barebox-version"),
            baseboard_release: {
                let template = read_dt_property("baseboard-factory-data/pcba-hardware-release");
                let changeset = read_dt_property_u32("baseboard-factory-data/modification");

                template.replace("-C??", &format!("-C{changeset:02}"))
            },
            powerboard_release: {
                let template = read_dt_property("powerboard-factory-data/pcba-hardware-release");
                let changeset = read_dt_property_u32("powerboard-factory-data/modification");

                template.replace("-C??", &format!("-C{changeset:02}"))
            },
            baseboard_timestamp: {
                read_dt_property_u32("baseboard-factory-data/factory-timestamp")
            },
            powerboard_timestamp: {
                read_dt_property_u32("powerboard-factory-data/factory-timestamp")
            },
        }
    }
}

pub struct System {
    pub uname: Arc<Topic<Arc<Uname>>>,
    pub barebox: Arc<Topic<Arc<Barebox>>>,
    pub tacd_version: Arc<Topic<String>>,
}

impl System {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        let version = env!("VERSION_STRING").to_string();

        Self {
            uname: bb.topic_ro("/v1/tac/info/uname", Some(Arc::new(Uname::get()))),
            barebox: bb.topic_ro("/v1/tac/info/bootloader", Some(Arc::new(Barebox::get()))),
            tacd_version: bb.topic_ro("/v1/tac/info/tacd/version", Some(version)),
        }
    }
}
