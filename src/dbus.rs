use async_std::sync::Arc;

use crate::broker::BrokerBuilder;

#[cfg(feature = "stub_out_dbus")]
mod zb {
    pub type Result<T> = std::result::Result<T, ()>;

    pub struct Connection;
    pub struct ConnectionBuilder;

    impl ConnectionBuilder {
        pub fn system() -> Result<Self> {
            Ok(Self)
        }

        pub fn name(self, _: &'static str) -> Result<Self> {
            Ok(self)
        }

        pub fn serve_at<T>(self, _: &'static str, _: T) -> Result<Self> {
            Ok(self)
        }

        pub async fn build(self) -> Result<Connection> {
            Ok(Connection)
        }
    }
}

#[cfg(not(feature = "stub_out_dbus"))]
mod zb {
    pub use zbus::*;
}

use zb::{Connection, ConnectionBuilder, Result};

mod networkmanager;
mod rauc;
mod systemd;
mod tacd;

use self::systemd::Systemd;
pub use networkmanager::{LinkInfo, Network};
pub use rauc::{Progress, Rauc};
pub use tacd::Tacd;

/// Bunch together everything that uses a DBus system connection here, even
/// though it is conceptionally independent
pub struct DbusSession {
    pub network: Network,
    pub rauc: Rauc,
    pub system: Systemd,
}

impl DbusSession {
    pub async fn new(bb: &mut BrokerBuilder) -> Self {
        let tacd = Tacd::new();

        let conn_builder = ConnectionBuilder::system()
            .unwrap()
            .name("de.pengutronix.tacd")
            .unwrap();

        let conn = Arc::new(tacd.serve(conn_builder).build().await.unwrap());

        Self {
            network: Network::new(bb, &conn).await,
            rauc: Rauc::new(bb, &conn).await,
            system: Systemd::new(bb, &conn).await,
        }
    }
}
