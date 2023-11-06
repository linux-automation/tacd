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
    pub(super) use futures::{future::FutureExt, pin_mut, select};
    pub(super) use log::trace;
    pub(super) use std::convert::TryInto;
    pub(super) use std::time::Duration;
    pub(super) use zbus::{Connection, PropertyStream};
    pub(super) use zvariant::{ObjectPath, OwnedObjectPath};

    pub(super) use super::devices::{DeviceProxy, WiredProxy};
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
async fn path_from_interface(con: &Connection, interface: &str) -> Result<OwnedObjectPath> {
    let proxy = NetworkManagerProxy::new(con).await?;
    let device_paths = proxy.get_devices().await?;

    for path in device_paths {
        let device_proxy = DeviceProxy::builder(con).path(&path)?.build().await?;

        let interface_name = device_proxy.interface().await?; // name

        // Is this the interface we are interested in?
        if interface_name == interface {
            return Ok(path);
        }
    }
    Err(anyhow!("No interface found: {}", interface))
}

#[cfg(not(feature = "demo_mode"))]
async fn get_link_info(con: &Connection, path: &str) -> Result<LinkInfo> {
    let eth_proxy = WiredProxy::builder(con).path(path)?.build().await?;

    let speed = eth_proxy.speed().await?;
    let carrier = eth_proxy.carrier().await?;

    let info = LinkInfo { speed, carrier };

    Ok(info)
}

#[cfg(not(feature = "demo_mode"))]
pub async fn get_ip4_address<'a, P>(con: &Connection, path: P) -> Result<Vec<String>>
where
    P: TryInto<ObjectPath<'a>>,
    P::Error: Into<zbus::Error>,
{
    let ip_4_proxy = IP4ConfigProxy::builder(con).path(path)?.build().await?;

    let ip_address = ip_4_proxy.address_data().await?;
    trace!("get IPv4: {:?}", ip_address);
    let ip_address = ip_address
        .get(0)
        .and_then(|e| e.get("address"))
        .and_then(|e| e.downcast_ref::<zvariant::Str>())
        .map(|e| e.as_str())
        .ok_or(anyhow!("IP not found"))?;
    Ok(Vec::from([ip_address.to_string()]))
}

#[cfg(not(feature = "demo_mode"))]
pub struct LinkStream<'a> {
    pub interface: String,
    _con: Arc<Connection>,
    speed: PropertyStream<'a, u32>,
    carrier: PropertyStream<'a, bool>,
    data: LinkInfo,
}

#[cfg(not(feature = "demo_mode"))]
impl<'a> LinkStream<'a> {
    pub async fn new(con: Arc<Connection>, interface: &str) -> Result<LinkStream<'a>> {
        let path = path_from_interface(&con, interface)
            .await?
            .as_str()
            .to_string();

        let eth_proxy = WiredProxy::builder(&con)
            .path(path.clone())?
            .build()
            .await?;

        let speed = eth_proxy.receive_speed_changed().await;
        let carrier = eth_proxy.receive_carrier_changed().await;

        let info = get_link_info(&con, path.as_str()).await?;

        Ok(Self {
            interface: interface.to_string(),
            _con: con,
            speed,
            carrier,
            data: info,
        })
    }

    pub fn now(&self) -> LinkInfo {
        self.data.clone()
    }

    pub async fn next(&mut self) -> Result<LinkInfo> {
        let speed = StreamExt::next(&mut self.speed).fuse();
        let carrier = StreamExt::next(&mut self.carrier).fuse();

        pin_mut!(speed, carrier);
        select! {
            speed2 = speed => {
                if let Some(s) = speed2 {
                    let s = s.get().await?;
                    trace!("update speed: {} {:?}", self.interface, s);
                    self.data.speed = s;
                }
            },
            carrier2 = carrier => {
                if let Some(c) = carrier2 {
                    let c = c.get().await?;
                    trace!("update carrier: {} {:?}", self.interface, c);
                    self.data.carrier = c;
                }
            },
        };
        Ok(self.data.clone())
    }
}

#[cfg(not(feature = "demo_mode"))]
pub struct IpStream<'a> {
    pub interface: String,
    _con: Arc<Connection>,
    ip_4_config: PropertyStream<'a, OwnedObjectPath>,
    path: String,
}

#[cfg(not(feature = "demo_mode"))]
impl<'a> IpStream<'a> {
    pub async fn new(con: Arc<Connection>, interface: &str) -> Result<IpStream<'a>> {
        let path = path_from_interface(&con, interface)
            .await?
            .as_str()
            .to_string();

        let device_proxy = DeviceProxy::builder(&con)
            .path(path.clone())?
            .build()
            .await?;

        let ip_4_config = device_proxy.receive_ip4_config_changed().await;

        Ok(Self {
            interface: interface.to_string(),
            _con: con,
            ip_4_config,
            path: path.to_string(),
        })
    }

    pub async fn now(&mut self, con: &Connection) -> Result<Vec<String>> {
        let device_proxy = DeviceProxy::builder(con)
            .path(self.path.as_str())?
            .build()
            .await?;

        let ip_4_config = device_proxy.ip4_config().await?;

        Ok(get_ip4_address(con, ip_4_config)
            .await
            .unwrap_or_else(|_e| Vec::new()))
    }

    pub async fn next(&mut self, con: &Connection) -> Result<Vec<String>> {
        let ip_4_config = StreamExt::next(&mut self.ip_4_config).await;

        if let Some(path) = ip_4_config {
            let path = path.get().await?;
            if let Ok(ips) = get_ip4_address(con, &path).await {
                trace!("updata ip: {} {:?}", self.interface, ips);
                return Ok(ips);
            } else {
                return Ok(Vec::new());
            }
        }
        Err(anyhow!("No IP found"))
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
            bridge_interface: bb.topic_ro("/v1/tac/network/interface/tac-bridge", None),
            dut_interface: bb.topic_ro("/v1/tac/network/interface/dut", None),
            uplink_interface: bb.topic_ro("/v1/tac/network/interface/uplink", None),
        }
    }

    #[cfg(feature = "demo_mode")]
    pub fn new<C>(
        bb: &mut BrokerBuilder,
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
        conn: &Arc<Connection>,
        led_dut: Arc<Topic<BlinkPattern>>,
        led_uplink: Arc<Topic<BlinkPattern>>,
    ) -> Self {
        let this = Self::setup_topics(bb);

        {
            let conn = conn.clone();
            let dut_interface = this.dut_interface.clone();
            async_std::task::spawn(async move {
                let mut link_stream = loop {
                    if let Ok(ls) = LinkStream::new(conn.clone(), "dut").await {
                        break ls;
                    }

                    sleep(Duration::from_secs(1)).await;
                };

                dut_interface.set(link_stream.now());

                while let Ok(info) = link_stream.next().await {
                    // The two color LED on the DUT interface is under the control of
                    // the switch IC. For 100MBit/s and 1GBit/s it lights in distinct
                    // colors, but for 10MBit/s it is just off.
                    // Build the most round-about link speed indicator ever so that we
                    // have speed indication for 10MBit/s.
                    let led_brightness = if info.speed == 10 { 1.0 } else { 0.0 };
                    led_dut.set(BlinkPattern::solid(led_brightness));

                    dut_interface.set(info);
                }
            });
        }

        {
            let conn = conn.clone();
            let uplink_interface = this.uplink_interface.clone();
            async_std::task::spawn(async move {
                let mut link_stream = loop {
                    if let Ok(ls) = LinkStream::new(conn.clone(), "uplink").await {
                        break ls;
                    }

                    sleep(Duration::from_secs(1)).await;
                };

                uplink_interface.set(link_stream.now());

                while let Ok(info) = link_stream.next().await {
                    // See the equivalent section on the uplink interface on why
                    // this is here.
                    let led_brightness = if info.speed == 10 { 1.0 } else { 0.0 };
                    led_uplink.set(BlinkPattern::solid(led_brightness));

                    uplink_interface.set(info);
                }
            });
        }

        {
            let conn = conn.clone();
            let bridge_interface = this.bridge_interface.clone();
            async_std::task::spawn(async move {
                let mut ip_stream = loop {
                    if let Ok(ips) = IpStream::new(conn.clone(), "tac-bridge").await {
                        break ips;
                    }

                    sleep(Duration::from_secs(1)).await;
                };

                bridge_interface.set(ip_stream.now(&conn).await.unwrap());

                while let Ok(info) = ip_stream.next(&conn).await {
                    bridge_interface.set(info);
                }
            });
        }

        this
    }
}
