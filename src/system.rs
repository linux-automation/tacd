use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;

use crate::broker::{BrokerBuilder, Topic};

use nix::sys::reboot::{reboot, RebootMode};

pub struct System {
    pub reboot: Arc<Topic<bool>>,
}

impl System {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        let reboot_topic = bb.topic_rw("/v1/tac/reboot");

        let reboot_task = reboot_topic.clone();
        spawn(async move {
            let (mut reboot_sub, _) = reboot_task.subscribe_unbounded().await;

            while let Some(ev) = reboot_sub.next().await {
                if *ev {
                    let _ = reboot(RebootMode::RB_AUTOBOOT);
                }
            }
        });

        Self {
            reboot: reboot_topic,
        }
    }
}
