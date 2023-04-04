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

use async_std::sync::Arc;

use crate::broker::{BrokerBuilder, Topic};
use crate::led::BlinkPattern;

#[cfg(feature = "demo_mode")]
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

#[cfg(not(feature = "demo_mode"))]
mod zb {
    pub use zbus::*;
}

use zb::{Connection, ConnectionBuilder, Result};

pub mod networkmanager;
pub mod rauc;
pub mod systemd;
pub mod tacd;

pub use self::systemd::Systemd;
pub use networkmanager::Network;
pub use rauc::Rauc;
pub use tacd::Tacd;

/// Bunch together everything that uses a DBus system connection here, even
/// though it is conceptionally independent
pub struct DbusSession {
    pub network: Network,
    pub rauc: Rauc,
    pub systemd: Systemd,
}

impl DbusSession {
    pub async fn new(
        bb: &mut BrokerBuilder,
        led_dut: Arc<Topic<BlinkPattern>>,
        led_uplink: Arc<Topic<BlinkPattern>>,
    ) -> Self {
        let tacd = Tacd::new();

        let conn_builder = ConnectionBuilder::system()
            .unwrap()
            .name("de.pengutronix.tacd")
            .unwrap();

        let conn = Arc::new(tacd.serve(conn_builder).build().await.unwrap());

        Self {
            network: Network::new(bb, &conn, led_dut, led_uplink),
            rauc: Rauc::new(bb, &conn).await,
            systemd: Systemd::new(bb, &conn).await,
        }
    }
}
