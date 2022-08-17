use async_std::sync::Arc;
use nix::sys::utsname::uname;
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

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

pub struct System {
    pub uname: Arc<Topic<Uname>>,
}

impl System {
    pub async fn new(bb: &mut BrokerBuilder) -> Self {
        let sys = Self {
            uname: bb.topic_ro("/v1/tac/uname"),
        };

        sys.uname.set(Uname::get()).await;

        sys
    }
}
