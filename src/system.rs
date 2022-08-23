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
        u32::from_str_radix(&read_dt_property(path), 10).unwrap()
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
        // Get info from devicetree choosen
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
    pub uname: Arc<Topic<Uname>>,
    pub barebox: Arc<Topic<Barebox>>,
}

impl System {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        Self {
            uname: bb.topic_ro("/v1/tac/uname", Some(Uname::get())),
            barebox: bb.topic_ro("/v1/tac/bootloader", Some(Barebox::get())),
        }
    }
}
