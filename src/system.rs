use std::fs::read;
use std::str::from_utf8;

use async_std::sync::Arc;
use nix::sys::utsname::uname;
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

const DT_CHOSEN: &str = "/sys/firmware/devicetree/base/chosen/";

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
    fn read_property(path: &str) -> String {
        let bytes = read([DT_CHOSEN, path].join("/")).unwrap();
        from_utf8(bytes.strip_suffix(&[0]).unwrap())
            .unwrap()
            .to_string()
    }

    fn get() -> Self {
        // Get info from devicetree choosen
        Self {
            version: Self::read_property("barebox-version"),
            baseboard_release: {
                let template = Self::read_property("baseboard-factory-data/pcba-hardware-release");
                let changeset = Self::read_property("baseboard-factory-data/modification");
                let changeset = u32::from_str_radix(&changeset, 10).unwrap();

                template.replace("-C??", &format!("-C{changeset:02}"))
            },
            powerboard_release: {
                let template = Self::read_property("powerboard-factory-data/pcba-hardware-release");
                let changeset = Self::read_property("powerboard-factory-data/modification");
                let changeset = u32::from_str_radix(&changeset, 10).unwrap();

                template.replace("-C??", &format!("-C{changeset:02}"))
            },
            baseboard_timestamp: {
                let ts = Self::read_property("baseboard-factory-data/factory-timestamp");
                u32::from_str_radix(&ts, 10).unwrap()
            },
            powerboard_timestamp: {
                let ts = Self::read_property("powerboard-factory-data/factory-timestamp");
                u32::from_str_radix(&ts, 10).unwrap()
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
