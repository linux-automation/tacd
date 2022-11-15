use anyhow;
use async_std;
use async_std::stream::StreamExt;
use async_std::sync::Arc;
use futures::{future::FutureExt, pin_mut, select};
use zbus::{Connection, PropertyStream};
use zvariant::{ObjectPath, OwnedObjectPath};
mod devices;
mod networkmanager;
use log::trace;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkInfo {
    pub speed: u32,
    pub carrier: bool,
}

impl Default for LinkInfo {
    fn default() -> Self {
        Self {
            speed: 0,
            carrier: false,
        }
    }
}

async fn path_from_interface(con: &Connection, interface: &str) -> anyhow::Result<OwnedObjectPath> {
    let proxy = networkmanager::NetworkManagerProxy::new(&con).await?;
    let device_paths = proxy.get_devices().await?;

    for path in device_paths {
        let device_proxy = devices::DeviceProxy::builder(&con)
            .path(&path)?
            .build()
            .await?;

        let interface_name = device_proxy.interface().await?; // name

        // Is this the interface we are interrested in?
        if interface_name == interface {
            return Ok(path);
        }
    }
    Err(anyhow::anyhow!("No interface found: {}", interface))
}

async fn get_link_info(con: &Connection, path: &str) -> anyhow::Result<LinkInfo> {
    let eth_proxy = devices::WiredProxy::builder(&con)
        .path(path)?
        .build()
        .await?;

    let speed = eth_proxy.speed().await?;
    let carrier = eth_proxy.carrier().await?;

    let info = LinkInfo { speed, carrier };

    Ok(info)
}

pub async fn get_ip4_address<'a, P>(con: &Connection, path: P) -> anyhow::Result<Vec<String>>
where
    P: TryInto<ObjectPath<'a>>,
    P::Error: Into<zbus::Error>,
{
    let ip_4_proxy = devices::ip4::IP4ConfigProxy::builder(&con)
        .path(path)?
        .build()
        .await?;

    let ip_address = ip_4_proxy.address_data2().await?;
    trace!("get IPv4: {:?}", ip_address);
    let ip_address = ip_address
        .get(0)
        .and_then(|e| e.get("address"))
        .and_then(|e| e.downcast_ref::<zvariant::Str>())
        .and_then(|e| Some(e.as_str()))
        .ok_or(anyhow::anyhow!("IP not found"))?;
    Ok(Vec::from([ip_address.to_string()]))
}

pub struct LinkStream<'a> {
    pub interface: String,
    _con: Arc<Connection>,
    speed: PropertyStream<'a, u32>,
    carrier: PropertyStream<'a, bool>,
    data: LinkInfo,
}

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
            speed: speed,
            carrier: carrier,
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

pub struct IpStream<'a> {
    pub interface: String,
    _con: Arc<Connection>,
    ip_4_config: PropertyStream<'a, OwnedObjectPath>,
    path: String,
}

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
            ip_4_config: ip_4_config,
            path: path.to_string(),
        })
    }

    pub async fn now(&mut self, con: &Connection) -> anyhow::Result<Vec<String>> {
        let device_proxy = devices::DeviceProxy::builder(&con)
            .path(self.path.as_str())?
            .build()
            .await?;

        let ip_4_config = device_proxy.ip4_config().await?;

        if let Ok(ips) = get_ip4_address(con, ip_4_config).await {
            return Ok(ips);
        } else {
            return Ok(Vec::new());
        }
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
