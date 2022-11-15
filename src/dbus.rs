use async_std::sync::Arc;
use async_std::task::spawn;
use zbus::dbus_proxy;

use crate::broker::{BrokerBuilder, Topic};

mod networkmanager;

pub use networkmanager::{IpStream, LinkInfo, LinkStream};

#[dbus_proxy(
    default_service = "org.freedesktop.hostname1",
    interface = "org.freedesktop.hostname1",
    default_path = "/org/freedesktop/hostname1"
)]
trait Hostname {
    #[dbus_proxy(property)]
    fn hostname(&self) -> zbus::Result<String>;
}

pub struct DbusClient {
    pub hostname: Arc<Topic<String>>,
    pub bridge_interface: Arc<Topic<Vec<String>>>,
    pub dut_interface: Arc<Topic<LinkInfo>>,
    pub uplink_interface: Arc<Topic<LinkInfo>>,
}

impl DbusClient {
    pub async fn new(bb: &mut BrokerBuilder) -> Self {
        let conn = Arc::new(zbus::Connection::system().await.unwrap());

        let hostname = {
            let topic = bb.topic_ro("/v1/tac/network/hostname");
            let manager = HostnameProxy::new(&conn).await.unwrap();
            let hostname = manager.hostname().await.unwrap();
            topic.set(hostname).await;
            topic
        };

        let bridge_interface = bb.topic_ro("/v1/tac/network/tac-bridge");
        let dut_interface = bb.topic_ro("/v1/tac/network/dut");
        let uplink_interface = bb.topic_ro("/v1/tac/network/uplink");

        {
            let conn = conn.clone();
            let mut nm_interface = LinkStream::new(conn, "dut").await.unwrap();
            dut_interface.set(nm_interface.now()).await;

            let dut_interface = dut_interface.clone();
            spawn(async move {
                while let Ok(info) = nm_interface.next().await {
                    dut_interface.set(info).await;
                }
            });
        }

        {
            let conn = conn.clone();
            let mut nm_interface = LinkStream::new(conn, "uplink").await.unwrap();
            uplink_interface.set(nm_interface.now()).await;

            let uplink_interface = uplink_interface.clone();
            spawn(async move {
                while let Ok(info) = nm_interface.next().await {
                    uplink_interface.set(info).await;
                }
            });
        }

        {
            let mut nm_interface = IpStream::new(conn.clone(), "tac-bridge").await.unwrap();
            bridge_interface
                .set(nm_interface.now(&conn).await.unwrap())
                .await;

            let bridge_interface = bridge_interface.clone();
            spawn(async move {
                while let Ok(info) = nm_interface.next(&conn).await {
                    bridge_interface.set(info).await;
                }
            });
        }

        Self {
            hostname,
            bridge_interface,
            dut_interface,
            uplink_interface,
        }
    }
}
