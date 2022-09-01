use std::fs::{read_to_string, write};
use std::path::Path;
use std::time::Duration;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{sleep, spawn};
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

const PORTS: &[(&str, &str)] = &[
    (
        "port1",
        "/sys/devices/platform/soc/5800d000.usb/usb1/1-1/1-1:1.0/1-1-port1",
    ),
    (
        "port2",
        "/sys/devices/platform/soc/5800d000.usb/usb1/1-1/1-1:1.0/1-1-port2",
    ),
    (
        "port3",
        "/sys/devices/platform/soc/5800d000.usb/usb1/1-1/1-1:1.0/1-1-port3",
    ),
];

#[derive(Serialize, Deserialize, PartialEq)]
pub struct UsbDevice {
    id_product: String,
    id_vendor: String,
    manufacturer: String,
    product: String,
}

#[derive(Clone)]
pub struct UsbPort {
    pub powered: Arc<Topic<bool>>,
    pub device: Arc<Topic<Option<UsbDevice>>>,
}

pub struct UsbHub {
    pub port1: UsbPort,
    pub port2: UsbPort,
    pub port3: UsbPort,
}

fn handle_port(bb: &mut BrokerBuilder, name: &'static str, base: &'static str) -> UsbPort {
    let port = UsbPort {
        powered: bb.topic_rw(format!("/v1/usb/host/{name}/powered").as_str(), Some(true)),
        device: bb.topic_ro(format!("/v1/usb/host/{name}/device").as_str(), Some(None)),
    };

    let powered = port.powered.clone();
    let device = port.device.clone();
    let disable_path = Path::new(base).join("disable");

    // Spawn a task that turns USB port power on or off upon request.
    // Also clears the device info upon power off so it does not contain stale
    // information until the next poll.
    spawn(async move {
        let (mut src, _) = powered.subscribe_unbounded().await;

        while let Some(ev) = src.next().await.as_deref() {
            write(&disable_path, if *ev { b"0" } else { b"1" }).unwrap();

            if !*ev {
                device.set(None).await;
            }
        }
    });

    let device = port.device.clone();
    let (id_product_path, id_vendor_path, manufacturer_path, product_path) = {
        let device_path = Path::new(base).join("device");
        (
            device_path.join("idProduct"),
            device_path.join("idVendor"),
            device_path.join("manufacturer"),
            device_path.join("product"),
        )
    };

    // Spawn a task that periodically polls the USB device info and updates
    // the corresponding topic on changes.
    //
    // TODO: also check disable status to make sure the state stays consistent
    // even when e.g. uhubctl is used?
    spawn(async move {
        loop {
            let id_product = read_to_string(&id_product_path).ok();
            let id_vendor = read_to_string(&id_vendor_path).ok();
            let manufacturer = read_to_string(&manufacturer_path).ok();
            let product = read_to_string(&product_path).ok();

            let ids = id_product.zip(id_vendor);
            let strings = manufacturer.zip(product);

            let dev_info = ids.zip(strings).map(|((idp, idv), (man, pro))| UsbDevice {
                id_product: idp.trim().to_string(),
                id_vendor: idv.trim().to_string(),
                manufacturer: man.trim().to_string(),
                product: pro.trim().to_string(),
            });

            device
                .modify(|prev| {
                    let should_set = prev
                        .map(|prev_dev_info| *prev_dev_info != dev_info)
                        .unwrap_or(true);

                    if should_set {
                        Some(Arc::new(dev_info))
                    } else {
                        None
                    }
                })
                .await;

            sleep(POLL_INTERVAL).await;
        }
    });

    port
}

impl UsbHub {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        let mut ports = PORTS.iter().map(|(name, base)| handle_port(bb, name, base));

        Self {
            port1: ports.next().unwrap(),
            port2: ports.next().unwrap(),
            port3: ports.next().unwrap(),
        }
    }
}
