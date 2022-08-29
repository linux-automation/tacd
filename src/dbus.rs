use async_std::sync::Arc;

use crate::broker::BrokerBuilder;

mod networkmanager;
mod rauc;
mod systemd;

use self::systemd::Systemd;
pub use networkmanager::{LinkInfo, Network};
pub use rauc::{Progress, Rauc};

/// Bunch together everything that uses a DBus system connection here, even
/// though it is conceptionally independent
pub struct DbusClient {
    pub network: Network,
    pub system: Systemd,
    pub rauc: Rauc,
}

impl DbusClient {
    pub async fn new(bb: &mut BrokerBuilder) -> Self {
        #[cfg(not(feature = "stub_out_dbus"))]
        let conn = Arc::new(zbus::Connection::system().await.unwrap());

        #[cfg(feature = "stub_out_dbus")]
        let conn = ();

        Self {
            network: Network::new(bb, &conn).await,
            system: Systemd::new(bb, &conn).await,
            rauc: Rauc::new(bb, &conn).await,
        }
    }
}
