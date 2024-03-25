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

use anyhow::{anyhow, Result};
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::sleep;
use serde::{Deserialize, Serialize};

use crate::adc::CalibratedChannel;
use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
mod rw {
    use std::collections::HashMap;
    use std::io::Result;
    use std::path::Path;
    use std::sync::Mutex;

    use async_std::task::block_on;

    use crate::adc::IioThread;

    const DEVICES: &[(&str, &str)] = &[
        ("/1-1-port1/device/idProduct", "1234"),
        ("/1-1-port1/device/idVendor", "33f7"),
        ("/1-1-port1/device/manufacturer", "Linux Automation GmbH"),
        ("/1-1-port1/device/product", "Christmas Tree Ornament"),
        ("/1-1-port2/device/idProduct", "4321"),
        ("/1-1-port2/device/idVendor", "33f7"),
        ("/1-1-port2/device/manufacturer", "Linux Automation GmbH"),
        ("/1-1-port2/device/product", "LXA Water Hose Mux"),
        ("/1-1-port3/device/idProduct", "cafe"),
        ("/1-1-port3/device/idVendor", "33f7"),
        ("/1-1-port3/device/manufacturer", "Linux Automation GmbH"),
        ("/1-1-port3/device/product", "Mug warmer"),
    ];

    const DISABLE_CHANNELS: &[(&str, &str)] = &[
        ("/1-1-port1/disable", "usb-host1-curr"),
        ("/1-1-port2/disable", "usb-host2-curr"),
        ("/1-1-port3/disable", "usb-host3-curr"),
    ];

    static FILESYSTEM: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

    pub(super) fn read_to_string<P: AsRef<Path>>(path: P) -> Result<String> {
        let path = path.as_ref().to_str().unwrap();

        if let Some(stored) = FILESYSTEM
            .lock()
            .unwrap()
            .get_or_insert(HashMap::new())
            .get(path)
            .cloned()
        {
            return Ok(stored);
        }

        for (path_tail, content) in DEVICES {
            if path.ends_with(path_tail) {
                return Ok(content.to_string());
            }
        }

        Ok("0".to_string())
    }

    pub(super) fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
        let path: &Path = path.as_ref();
        let path = path.to_str().unwrap().to_string();
        let contents: &[u8] = contents.as_ref();
        let text = std::str::from_utf8(contents)
            .unwrap_or("[Broken UTF-8]")
            .to_string();

        for (path_tail, iio_channel) in DISABLE_CHANNELS {
            if path.ends_with(path_tail) {
                let iio_thread = block_on(IioThread::new_stm32(&())).unwrap();

                iio_thread
                    .get_channel(iio_channel)
                    .unwrap()
                    .set(text == "0");
            }
        }

        FILESYSTEM
            .lock()
            .unwrap()
            .get_or_insert(HashMap::new())
            .insert(path, text);

        Ok(())
    }
}

#[cfg(not(feature = "demo_mode"))]
mod rw {
    pub(super) use std::fs::*;
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

// The total current for all ports is limited to 700mA, the per-port current is
// limited to 500mA.
pub const MAX_TOTAL_CURRENT: f32 = 0.7;
pub const MAX_PORT_CURRENT: f32 = 0.5;

// The measurement is not _that_ exact so start warning at 90% utilization.
const CURRENT_MARGIN: f32 = 0.9;
const WARN_TOTAL_CURRENT: f32 = MAX_TOTAL_CURRENT * CURRENT_MARGIN;
const WARN_PORT_CURRENT: f32 = MAX_PORT_CURRENT * CURRENT_MARGIN;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum OverloadedPort {
    Total,
    Port1,
    Port2,
    Port3,
}

impl OverloadedPort {
    fn from_currents(total: f32, port1: f32, port2: f32, port3: f32) -> Option<Self> {
        // Based on the maximum / per-port limits it should not be possible for two
        // individual ports to be overloaded at the same time while the total is not
        // overloaded, so reporting either "total" or one of the ports should be
        // sufficient.

        if total > WARN_TOTAL_CURRENT {
            Some(Self::Total)
        } else if port1 > WARN_PORT_CURRENT {
            Some(Self::Port1)
        } else if port2 > WARN_PORT_CURRENT {
            Some(Self::Port2)
        } else if port3 > WARN_PORT_CURRENT {
            Some(Self::Port3)
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct UsbDevice {
    id_product: String,
    id_vendor: String,
    manufacturer: String,
    product: String,
}

#[derive(Clone)]
pub struct UsbPort {
    pub request: Arc<Topic<bool>>,
    pub status: Arc<Topic<bool>>,
    pub device: Arc<Topic<Option<UsbDevice>>>,
}

pub struct UsbHub {
    pub overload: Arc<Topic<Option<OverloadedPort>>>,
    pub port1: UsbPort,
    pub port2: UsbPort,
    pub port3: UsbPort,
}

fn handle_port(
    bb: &mut BrokerBuilder,
    wtb: &mut WatchedTasksBuilder,
    name: &'static str,
    base: &'static str,
) -> Result<UsbPort> {
    let port = UsbPort {
        request: bb.topic_wo(format!("/v1/usb/host/{name}/powered").as_str(), None),
        status: bb.topic_ro(format!("/v1/usb/host/{name}/powered").as_str(), None),
        device: bb.topic_ro(format!("/v1/usb/host/{name}/device").as_str(), Some(None)),
    };

    let request = port.request.clone();
    let status = port.status.clone();
    let device = port.device.clone();
    let disable_path = Path::new(base).join("disable");

    // Spawn a task that turns USB port power on or off upon request.
    // Also clears the device info upon power off so it does not contain stale
    // information until the next poll.
    wtb.spawn_task(format!("usb-hub-{name}-actions"), async move {
        let (mut src, _) = request.subscribe_unbounded();

        while let Some(ev) = src.next().await {
            write(&disable_path, if ev { b"0" } else { b"1" })?;

            if !ev {
                device.set(None);
            }

            status.set(ev);
        }

        Ok(())
    })?;

    let status = port.status.clone();
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
    wtb.spawn_task(format!("usb-hub-{name}-state"), async move {
        loop {
            if let Ok(disable) = read_to_string(&disable_path) {
                let is_powered = match disable.trim() {
                    "1" => false,
                    "0" => true,
                    _ => panic!("Read unexpected value for USB port disable state"),
                };

                status.set_if_changed(is_powered);
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

            device.set_if_changed(dev_info);

            sleep(POLL_INTERVAL).await;
        }
    })?;

    Ok(port)
}

fn handle_overloads(
    bb: &mut BrokerBuilder,
    wtb: &mut WatchedTasksBuilder,
    total: CalibratedChannel,
    port1: CalibratedChannel,
    port2: CalibratedChannel,
    port3: CalibratedChannel,
) -> Result<Arc<Topic<Option<OverloadedPort>>>> {
    let overload = bb.topic_ro("/v1/usb/host/overload", None);

    let overload_task = overload.clone();

    wtb.spawn_task("usb-hub-overload-state", async move {
        loop {
            let overloaded_port = OverloadedPort::from_currents(
                total.get().map(|m| m.value).unwrap_or(0.0),
                port1.get().map(|m| m.value).unwrap_or(0.0),
                port2.get().map(|m| m.value).unwrap_or(0.0),
                port3.get().map(|m| m.value).unwrap_or(0.0),
            );

            overload_task.set_if_changed(overloaded_port);

            sleep(POLL_INTERVAL).await;
        }
    })?;

    Ok(overload)
}

impl UsbHub {
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        total: CalibratedChannel,
        port1: CalibratedChannel,
        port2: CalibratedChannel,
        port3: CalibratedChannel,
    ) -> Result<Self> {
        let overload = handle_overloads(bb, wtb, total, port1, port2, port3)?;

        let mut ports = PORTS
            .iter()
            .map(|(name, base)| handle_port(bb, wtb, name, base));

        Ok(Self {
            overload,
            port1: ports
                .next()
                .ok_or_else(|| anyhow!("Failed to find USB port 1"))??,
            port2: ports
                .next()
                .ok_or_else(|| anyhow!("Failed to find USB port 2"))??,
            port3: ports
                .next()
                .ok_or_else(|| anyhow!("Failed to find USB port 3"))??,
        })
    }
}
