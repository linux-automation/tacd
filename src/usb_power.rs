use std::fs::write;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;

use crate::broker::{BrokerBuilder, Topic};

const DISABLE_PATHS: &[&str] = &[
    "/sys/devices/platform/soc/5800d000.usb/usb1/1-1/1-1:1.0/1-1-port1/disable",
    "/sys/devices/platform/soc/5800d000.usb/usb1/1-1/1-1:1.0/1-1-port2/disable",
    "/sys/devices/platform/soc/5800d000.usb/usb1/1-1/1-1:1.0/1-1-port3/disable",
];

pub struct UsbPower {
    pub port1: Arc<Topic<bool>>,
    pub port2: Arc<Topic<bool>>,
    pub port3: Arc<Topic<bool>>,
}

fn handle_topic(disable: &'static str, topic: Arc<Topic<bool>>) {
    spawn(async move {
        topic.set(true).await;

        let (mut src, _) = topic.subscribe_unbounded().await;

        while let Some(ev) = src.next().await.as_deref() {
            write(disable, if *ev { b"0" } else { b"1" }).unwrap()
        }
    });
}

impl UsbPower {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        let usb = Self {
            port1: bb.topic_rw("/v1/usb/host/port1/powered"),
            port2: bb.topic_rw("/v1/usb/host/port2/powered"),
            port3: bb.topic_rw("/v1/usb/host/port3/powered"),
        };

        for (dis, topic) in
            DISABLE_PATHS
                .iter()
                .zip(&[usb.port1.clone(), usb.port2.clone(), usb.port3.clone()])
        {
            handle_topic(dis, topic.clone())
        }

        usb
    }
}
