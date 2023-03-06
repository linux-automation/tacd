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

use std::path::Path;
use std::time::Duration;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{sleep, spawn};
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

#[cfg(feature = "demo_mode")]
mod rw {
    use std::collections::HashMap;
    use std::convert::AsRef;
    use std::io::Result;
    use std::path::Path;
    use std::sync::Mutex;

    static FILESYSTEM: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

    pub fn read_to_string<P: AsRef<Path>>(path: P) -> Result<String> {
        Ok(FILESYSTEM
            .lock()
            .unwrap()
            .get_or_insert(HashMap::new())
            .get(path.as_ref().to_str().unwrap())
            .cloned()
            .unwrap_or(String::from("0")))
    }

    pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
        let path: &Path = path.as_ref();
        let contents: &[u8] = contents.as_ref();
        let text = std::str::from_utf8(contents).unwrap_or("[Broken UTF-8]");

        FILESYSTEM
            .lock()
            .unwrap()
            .get_or_insert(HashMap::new())
            .insert(path.to_str().unwrap().to_string(), text.to_string());

        println!("USB: Would write {text} to {path:?} but don't feel like it");

        Ok(())
    }
}

#[cfg(not(feature = "demo_mode"))]
mod rw {
    pub use std::fs::*;
}

use rw::{read_to_string, write};

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

#[derive(Serialize, Deserialize, PartialEq, Clone)]
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
        powered: bb.topic_rw(format!("/v1/usb/host/{name}/powered").as_str(), None),
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

        while let Some(ev) = src.next().await {
            write(&disable_path, if ev { b"0" } else { b"1" }).unwrap();

            if !ev {
                device.set(None).await;
            }
        }
    });

    let powered = port.powered.clone();
    let device = port.device.clone();
    let disable_path = Path::new(base).join("disable");
    let (id_product_path, id_vendor_path, manufacturer_path, product_path) = {
        let device_path = Path::new(base).join("device");
        (
            device_path.join("idProduct"),
            device_path.join("idVendor"),
            device_path.join("manufacturer"),
            device_path.join("product"),
        )
    };

    // Spawn a task that periodically polls the USB device info and disable state
    // and updates the corresponding topic on changes.
    spawn(async move {
        loop {
            if let Ok(disable) = read_to_string(&disable_path) {
                let is_powered = match disable.trim() {
                    "1" => false,
                    "0" => true,
                    _ => panic!("Read unexpected value for USB port disable state"),
                };

                powered
                    .modify(|prev| {
                        let should_set = prev
                            .map(|prev_powered| prev_powered != is_powered)
                            .unwrap_or(true);

                        match should_set {
                            true => Some(is_powered),
                            false => None,
                        }
                    })
                    .await;
            }

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
                        .map(|prev_dev_info| prev_dev_info != dev_info)
                        .unwrap_or(true);

                    match should_set {
                        true => Some(dev_info),
                        false => None,
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
