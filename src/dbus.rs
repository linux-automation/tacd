use async_std::sync::Arc;

use crate::broker::BrokerBuilder;

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

        #[cfg(not(feature = "stub_out_dbus"))]
        let conn = {
            let conn = tacd
                .serve(
                    zbus::ConnectionBuilder::system()
                        .unwrap()
                        .name("de.pengutronix.tacd")
                        .unwrap(),
                )
                .build()
                .await
                .unwrap();

            Arc::new(conn)
        };

        #[cfg(feature = "stub_out_dbus")]
        let conn = ();

        Self {
            network: Network::new(bb, &conn).await,
            rauc: Rauc::new(bb, &conn).await,
            system: Systemd::new(bb, &conn).await,
        }
    }
}
