use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use zbus::dbus_proxy;

use crate::broker::{BrokerBuilder, Topic};

mod networkmanager;
mod rauc;
mod systemd;

use self::systemd::Systemd;
pub use networkmanager::{IpStream, LinkInfo, LinkStream};
pub use rauc::{Progress, Rauc};

#[dbus_proxy(
    default_service = "org.freedesktop.hostname1",
    interface = "org.freedesktop.hostname1",
    default_path = "/org/freedesktop/hostname1"
)]
trait Hostname {
    #[dbus_proxy(property)]
    fn hostname(&self) -> zbus::Result<String>;
}

pub struct Network {
    pub hostname: Arc<Topic<String>>,
    pub bridge_interface: Arc<Topic<Vec<String>>>,
    pub dut_interface: Arc<Topic<LinkInfo>>,
    pub uplink_interface: Arc<Topic<LinkInfo>>,
}

pub struct System {
    pub restart_service: Arc<Topic<String>>,
    pub reboot: Arc<Topic<bool>>,
}

/// Bunch together everything that uses a DBus system connection here, even
/// though it is conceptionally independent
pub struct DbusClient {
    pub network: Network,
    pub system: System,
    pub rauc: Rauc,
}

impl DbusClient {
    pub async fn new(bb: &mut BrokerBuilder) -> Self {
        #[cfg(not(feature = "stub_out_dbus"))]
        let conn = Arc::new(zbus::Connection::system().await.unwrap());

        let hostname = {
            #[cfg(not(feature = "stub_out_dbus"))]
            let hostname = HostnameProxy::new(&conn)
                .await
                .unwrap()
                .hostname()
                .await
                .unwrap();

            #[cfg(feature = "stub_out_dbus")]
            let hostname = "lxatac".to_string();

            bb.topic_ro("/v1/tac/network/hostname", Some(hostname))
        };

        let bridge_interface = bb.topic_ro("/v1/tac/network/tac-bridge", None);
        let dut_interface = bb.topic_ro("/v1/tac/network/dut", None);
        let uplink_interface = bb.topic_ro("/v1/tac/network/uplink", None);

        #[cfg(not(feature = "stub_out_dbus"))]
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

        #[cfg(not(feature = "stub_out_dbus"))]
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

        #[cfg(not(feature = "stub_out_dbus"))]
        {
            let conn = conn.clone();
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

        #[cfg(not(feature = "stub_out_dbus"))]
        let sd = Systemd::new(conn.clone());

        let reboot = bb.topic_rw("/v1/tac/reboot", Some(false));

        #[cfg(not(feature = "stub_out_dbus"))]
        {
            let sd = sd.clone();
            let (mut reboot_reqs, _) = reboot.clone().subscribe_unbounded().await;

            spawn(async move {
                while let Some(req) = reboot_reqs.next().await {
                    if *req {
                        let _ = sd.reboot().await;
                    }
                }
            });
        }

        let restart_service =
            bb.topic_rw::<String>("/v1/tac/restart_service", Some("".to_string()));

        #[cfg(not(feature = "stub_out_dbus"))]
        {
            let sd = sd.clone();
            let (mut restart_reqs, _) = restart_service.clone().subscribe_unbounded().await;

            spawn(async move {
                while let Some(name) = restart_reqs.next().await {
                    let _ = sd.restart_service(name.as_str());
                }
            });
        }

        // TODO: this is arguably prettier than what is done above for network
        // and systemd. Maybe also push back the broker framework into those
        // modules.
        #[cfg(not(feature = "stub_out_dbus"))]
        let rauc = Rauc::new(bb, conn).await;

        #[cfg(feature = "stub_out_dbus")]
        let rauc = Rauc::new(bb).await;

        Self {
            network: Network {
                hostname,
                bridge_interface,
                dut_interface,
                uplink_interface,
            },
            system: System {
                reboot,
                restart_service,
            },
            rauc,
        }
    }
}
