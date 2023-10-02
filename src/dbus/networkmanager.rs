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

use async_std;
use async_std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};
use crate::led::BlinkPattern;
use crate::watched_tasks::WatchedTasksBuilder;

// Macro use makes these modules quite heavy, so we keep them commented
// out until they are actually used
//mod active_connection;
mod devices;
//mod dhcp4_config;
//mod dhcp6_config;
mod ipv4_config;
//mod ipv6_config;
mod manager;
//mod settings;

// All of the following includes are not used in demo_mode.
// Put them inside a mod so we do not have to decorate each one with
#[cfg(not(feature = "demo_mode"))]
mod optional_includes {
    pub(super) use anyhow::{anyhow, Result};
    pub(super) use async_std::stream::StreamExt;
    pub(super) use async_std::task::sleep;
    pub(super) use futures::{future::FutureExt, select};
    pub(super) use log::{info, trace};
    pub(super) use std::time::Duration;
    pub(super) use zbus::Connection;
    pub(super) use zvariant::OwnedObjectPath;

    pub(super) use super::devices::{DeviceProxy, WiredProxy, NM_DEVICE_STATE_ACTIVATED};
    pub(super) use super::ipv4_config::IP4ConfigProxy;
    pub(super) use super::manager::NetworkManagerProxy;
}

#[cfg(not(feature = "demo_mode"))]
use optional_includes::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkInfo {
    pub speed: u32,
    pub carrier: bool,
}

#[cfg(not(feature = "demo_mode"))]
async fn get_device_path(conn: &Arc<Connection>, interface_name: &str) -> OwnedObjectPath {
    let manager = loop {
        match NetworkManagerProxy::new(conn).await {
            Ok(m) => break m,
            Err(_e) => {
                info!("Failed to connect to NetworkManager via DBus. Retry in 1s");
            }
        }

        sleep(Duration::from_secs(1)).await;
    };

    loop {
        match manager.get_device_by_ip_iface(interface_name).await {
            Ok(d) => break d,
            Err(_e) => {
                info!("Failed to get interface {interface_name} from NetworkManager. Retry in 1s.")
            }
        }

        sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(not(feature = "demo_mode"))]
async fn handle_link_updates(
    conn: &Arc<Connection>,
    topic: Arc<Topic<LinkInfo>>,
    interface_name: &str,
    led: Arc<Topic<BlinkPattern>>,
) -> Result<()> {
    let device_path = get_device_path(conn, interface_name).await;
    let device = WiredProxy::builder(conn).path(device_path)?.build().await?;

    let mut carrier_changes = device.receive_carrier_changed().await;
    let mut speed_changes = device.receive_speed_changed().await;

    let mut info = LinkInfo {
        carrier: carrier_changes
            .next()
            .await
            .ok_or_else(|| anyhow!("Unexpected end of carrier subscription"))?
            .get()
            .await?,
        speed: speed_changes
            .next()
            .await
            .ok_or_else(|| anyhow!("Unexpected end of speed subscription"))?
            .get()
            .await?,
    };

    loop {
        // The two color LED on the DUT interface is under the control of
        // the switch IC. For 100MBit/s and 1GBit/s it lights in distinct
        // colors, but for 10MBit/s it is just off.
        // Build the most round-about link speed indicator ever so that we
        // have speed indication for 10MBit/s.
        led.set({
            let led_brightness = if info.speed == 10 { 1.0 } else { 0.0 };

            BlinkPattern::solid(led_brightness)
        });

        topic.set(info.clone());

        select! {
            carrier = carrier_changes.next().fuse() => {
                info.carrier = carrier
                    .ok_or_else(|| anyhow!("Unexpected end of carrier subscription"))?
                    .get()
                    .await?;
            }
            speed = speed_changes.next().fuse() => {
                info.speed = speed
                    .ok_or_else(|| anyhow!("Unexpected end of speed subscription"))?
                    .get()
                    .await?;
            }
        }
    }
}

#[cfg(not(feature = "demo_mode"))]
async fn handle_ipv4_updates(
    conn: &Arc<Connection>,
    topic: Arc<Topic<Vec<String>>>,
    interface_name: &str,
) -> Result<()> {
    let device_path = get_device_path(conn, interface_name).await;
    let device = DeviceProxy::builder(conn)
        .path(device_path)?
        .build()
        .await?;

    let mut state_changes = device.receive_state_property_changed().await;

    loop {
        // The NetworkManager DBus documentation says the Ip4Config property is
        // "Only valid when the device is in the NM_DEVICE_STATE_ACTIVATED state".
        // Loop until that is the case.
        'wait_activated: loop {
            let state = state_changes
                .next()
                .await
                .ok_or_else(|| anyhow!("Unexpected end of state change subscription"))?
                .get()
                .await?;

            trace!("Interface {interface_name} changed state to {state}");

            if state == NM_DEVICE_STATE_ACTIVATED {
                break 'wait_activated;
            }
        }

        let ip4_config_path = device.ip4_config().await?;
        let ip4_config = IP4ConfigProxy::builder(conn)
            .path(ip4_config_path)?
            .build()
            .await?;

        let mut address_data_changes = ip4_config.receive_address_data_changed().await;

        'wait_deactivated: loop {
            select! {
                new_state = state_changes.next().fuse() => {
                    let state = new_state
                        .ok_or_else(|| anyhow!("Unexpected end of state change subscription"))?
                        .get()
                        .await?;

                    trace!("Interface {interface_name} changed state to {state}");

                    topic.set(Vec::new());

                    if state != NM_DEVICE_STATE_ACTIVATED {
                        break 'wait_deactivated;
                    }
                }
                address_data = address_data_changes.next().fuse() => {
                    let address_data = address_data
                        .ok_or_else(|| anyhow!("Unexpected end of address data update stream"))?
                        .get()
                        .await?;

                    let addresses: Vec<String> = address_data
                        .iter()
                        .filter_map(|a| {
                            a.get("address")
                                .and_then(|e| e.downcast_ref::<zvariant::Str>())
                                .map(|e| e.as_str().to_owned())
                        })
                        .collect();

                    trace!("Interface {interface_name} got new IP addresses: {addresses:?}");

                    topic.set(addresses);
                }
            }
        }
    }
}

pub struct Network {
    pub bridge_interface: Arc<Topic<Vec<String>>>,
    pub dut_interface: Arc<Topic<LinkInfo>>,
    pub uplink_interface: Arc<Topic<LinkInfo>>,
}

impl Network {
    fn setup_topics(bb: &mut BrokerBuilder) -> Self {
        Self {
            bridge_interface: bb.topic_ro("/v1/tac/network/interface/tac-bridge", Some(Vec::new())),
            dut_interface: bb.topic_ro("/v1/tac/network/interface/dut", None),
            uplink_interface: bb.topic_ro("/v1/tac/network/interface/uplink", None),
        }
    }

    #[cfg(feature = "demo_mode")]
    pub fn new<C>(
        bb: &mut BrokerBuilder,
        _wtb: &mut WatchedTasksBuilder,
        _conn: C,
        _led_dut: Arc<Topic<BlinkPattern>>,
        _led_uplink: Arc<Topic<BlinkPattern>>,
    ) -> Self {
        let this = Self::setup_topics(bb);

        this.bridge_interface.set(vec![String::from("192.168.1.1")]);
        this.dut_interface.set(LinkInfo {
            speed: 0,
            carrier: false,
        });
        this.uplink_interface.set(LinkInfo {
            speed: 1000,
            carrier: true,
        });

        this
    }

    #[cfg(not(feature = "demo_mode"))]
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        conn: &Arc<Connection>,
        led_dut: Arc<Topic<BlinkPattern>>,
        led_uplink: Arc<Topic<BlinkPattern>>,
    ) -> Self {
        let this = Self::setup_topics(bb);

        let conn_task = conn.clone();
        let dut_interface = this.dut_interface.clone();
        wtb.spawn_task("link-dut-update", async move {
            handle_link_updates(&conn_task, dut_interface, "dut", led_dut).await
        });

        let conn_task = conn.clone();
        let uplink_interface = this.uplink_interface.clone();
        wtb.spawn_task("link-uplink-update", async move {
            handle_link_updates(&conn_task, uplink_interface, "uplink", led_uplink).await
        });

        let conn_task = conn.clone();
        let bridge_interface = this.bridge_interface.clone();
        wtb.spawn_task("ip-tac-bridge-update", async move {
            handle_ipv4_updates(&conn_task, bridge_interface, "tac-bridge").await
        });

        this
    }
}
