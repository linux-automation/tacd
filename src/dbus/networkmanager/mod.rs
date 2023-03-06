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

#[cfg(not(feature = "demo_mode"))]
use std::convert::TryInto;

#[cfg(not(feature = "demo_mode"))]
use anyhow;
use async_std;
#[cfg(not(feature = "demo_mode"))]
use async_std::stream::StreamExt;
use async_std::sync::Arc;
#[cfg(not(feature = "demo_mode"))]
use futures::{future::FutureExt, pin_mut, select};
#[cfg(not(feature = "demo_mode"))]
use zbus::{Connection, PropertyStream};
#[cfg(not(feature = "demo_mode"))]
use zvariant::{ObjectPath, OwnedObjectPath};

#[cfg(not(feature = "demo_mode"))]
use log::trace;
use serde::{Deserialize, Serialize};

mod devices;
mod hostname;

#[allow(clippy::module_inception)]
mod networkmanager;

use crate::broker::{BrokerBuilder, Topic};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkInfo {
    pub speed: u32,
    pub carrier: bool,
}

#[cfg(not(feature = "demo_mode"))]
async fn path_from_interface(con: &Connection, interface: &str) -> anyhow::Result<OwnedObjectPath> {
    let proxy = networkmanager::NetworkManagerProxy::new(con).await?;
    let device_paths = proxy.get_devices().await?;

    for path in device_paths {
        let device_proxy = devices::DeviceProxy::builder(con)
            .path(&path)?
            .build()
            .await?;

        let interface_name = device_proxy.interface().await?; // name

        // Is this the interface we are interested in?
        if interface_name == interface {
            return Ok(path);
        }
    }
    Err(anyhow::anyhow!("No interface found: {}", interface))
}

#[cfg(not(feature = "demo_mode"))]
async fn get_link_info(con: &Connection, path: &str) -> anyhow::Result<LinkInfo> {
    let eth_proxy = devices::WiredProxy::builder(con)
        .path(path)?
        .build()
        .await?;

    let speed = eth_proxy.speed().await?;
    let carrier = eth_proxy.carrier().await?;

    let info = LinkInfo { speed, carrier };

    Ok(info)
}

#[cfg(not(feature = "demo_mode"))]
pub async fn get_ip4_address<'a, P>(con: &Connection, path: P) -> anyhow::Result<Vec<String>>
where
    P: TryInto<ObjectPath<'a>>,
    P::Error: Into<zbus::Error>,
{
    let ip_4_proxy = devices::ip4::IP4ConfigProxy::builder(con)
        .path(path)?
        .build()
        .await?;

    let ip_address = ip_4_proxy.address_data2().await?;
    trace!("get IPv4: {:?}", ip_address);
    let ip_address = ip_address
        .get(0)
        .and_then(|e| e.get("address"))
        .and_then(|e| e.downcast_ref::<zvariant::Str>())
        .map(|e| e.as_str())
        .ok_or(anyhow::anyhow!("IP not found"))?;
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
    pub async fn new(con: Arc<Connection>, interface: &str) -> anyhow::Result<LinkStream<'a>> {
        let path = path_from_interface(&con, interface)
            .await?
            .as_str()
            .to_string();

        let eth_proxy = devices::WiredProxy::builder(&con)
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

    pub async fn next(&mut self) -> anyhow::Result<LinkInfo> {
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
    pub async fn new(con: Arc<Connection>, interface: &str) -> anyhow::Result<IpStream<'a>> {
        let path = path_from_interface(&con, interface)
            .await?
            .as_str()
            .to_string();

        let device_proxy = devices::DeviceProxy::builder(&con)
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

    pub async fn now(&mut self, con: &Connection) -> anyhow::Result<Vec<String>> {
        let device_proxy = devices::DeviceProxy::builder(con)
            .path(self.path.as_str())?
            .build()
            .await?;

        let ip_4_config = device_proxy.ip4_config().await?;

        Ok(get_ip4_address(con, ip_4_config)
            .await
            .unwrap_or_else(|_e| Vec::new()))
    }

    pub async fn next(&mut self, con: &Connection) -> anyhow::Result<Vec<String>> {
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
        Err(anyhow::anyhow!("No IP found"))
    }
}

pub struct Network {
    pub hostname: Arc<Topic<String>>,
    pub bridge_interface: Arc<Topic<Vec<String>>>,
    pub dut_interface: Arc<Topic<LinkInfo>>,
    pub uplink_interface: Arc<Topic<LinkInfo>>,
}

impl Network {
    fn setup_topics(bb: &mut BrokerBuilder, hostname: String) -> Self {
        Self {
            hostname: bb.topic_ro("/v1/tac/network/hostname", Some(hostname)),
            bridge_interface: bb.topic_ro("/v1/tac/network/interface/tac-bridge", None),
            dut_interface: bb.topic_ro("/v1/tac/network/interface/dut", None),
            uplink_interface: bb.topic_ro("/v1/tac/network/interface/uplink", None),
        }
    }

    #[cfg(feature = "demo_mode")]
    pub async fn new<C>(bb: &mut BrokerBuilder, _conn: C) -> Self {
        let this = Self::setup_topics(bb, "lxatac".to_string());

        this.bridge_interface
            .set(vec![String::from("192.168.1.1")])
            .await;
        this.dut_interface
            .set(LinkInfo {
                speed: 0,
                carrier: false,
            })
            .await;
        this.uplink_interface
            .set(LinkInfo {
                speed: 1000,
                carrier: true,
            })
            .await;

        this
    }

    #[cfg(not(feature = "demo_mode"))]
    pub async fn new(bb: &mut BrokerBuilder, conn: &Arc<Connection>) -> Self {
        let hostname = hostname::HostnameProxy::new(conn)
            .await
            .unwrap()
            .hostname()
            .await
            .unwrap();

        let this = Self::setup_topics(bb, hostname.to_string());

        {
            let conn = conn.clone();
            let mut nm_interface = LinkStream::new(conn, "dut").await.unwrap();
            let dut_interface = this.dut_interface.clone();
            async_std::task::spawn(async move {
                dut_interface.set(nm_interface.now()).await;

                while let Ok(info) = nm_interface.next().await {
                    dut_interface.set(info).await;
                }
            });
        }

        {
            let conn = conn.clone();
            let mut nm_interface = LinkStream::new(conn, "uplink").await.unwrap();
            let uplink_interface = this.uplink_interface.clone();
            async_std::task::spawn(async move {
                uplink_interface.set(nm_interface.now()).await;

                while let Ok(info) = nm_interface.next().await {
                    uplink_interface.set(info).await;
                }
            });
        }

        {
            let conn = conn.clone();
            let mut nm_interface = IpStream::new(conn.clone(), "tac-bridge").await.unwrap();
            let bridge_interface = this.bridge_interface.clone();
            async_std::task::spawn(async move {
                bridge_interface
                    .set(nm_interface.now(&conn).await.unwrap())
                    .await;

                while let Ok(info) = nm_interface.next(&conn).await {
                    bridge_interface.set(info).await;
                }
            });
        }

        this
    }
}
