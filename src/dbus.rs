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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use async_std::sync::Arc;

use crate::broker::{BrokerBuilder, Topic};
use crate::led::BlinkPattern;
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
mod zb {
    pub(super) use anyhow::Result;

    pub struct Connection;
    pub struct ConnectionBuilder;

    impl ConnectionBuilder {
        pub(super) fn system() -> Result<Self> {
            Ok(Self)
        }

        pub(super) fn name(self, _: &'static str) -> Result<Self> {
            Ok(self)
        }

        pub(super) fn serve_at<T>(self, _: &'static str, _: T) -> Result<Self> {
            Ok(self)
        }

        pub(super) async fn build(self) -> Result<Connection> {
            Ok(Connection)
        }
    }
}

#[cfg(not(feature = "demo_mode"))]
mod zb {
    pub(super) use zbus::Result;
    pub use zbus::{Connection, ConnectionBuilder};
}

use zb::{Connection, ConnectionBuilder, Result};

pub mod hostname;
pub mod networkmanager;
pub mod rauc;
pub mod systemd;
pub mod tacd;

pub use self::systemd::Systemd;
pub use hostname::Hostname;
pub use networkmanager::Network;
pub use rauc::Rauc;
pub use tacd::Tacd;

/// Bunch together everything that uses a DBus system connection here, even
/// though it is conceptually independent
pub struct DbusSession {
    pub hostname: Hostname,
    pub network: Network,
    pub rauc: Rauc,
    pub systemd: Systemd,
}

impl DbusSession {
    pub async fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        led_dut: Arc<Topic<BlinkPattern>>,
        led_uplink: Arc<Topic<BlinkPattern>>,
    ) -> anyhow::Result<Self> {
        let tacd = Tacd::new();

        let conn_builder = ConnectionBuilder::system()?.name("de.pengutronix.tacd")?;

        let conn = Arc::new(tacd.serve(conn_builder).build().await?);

        Ok(Self {
            hostname: Hostname::new(bb, wtb, &conn)?,
            network: Network::new(bb, wtb, &conn, led_dut, led_uplink)?,
            rauc: Rauc::new(bb, wtb, &conn)?,
            systemd: Systemd::new(bb, wtb, &conn).await?,
        })
    }
}
