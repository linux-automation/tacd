// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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
use async_std::stream::StreamExt;

#[cfg(not(feature = "demo_mode"))]
use zbus::Connection;

use crate::broker::{BrokerBuilder, Topic};
use async_std::sync::Arc;

mod hostnamed;

pub struct Hostname {
    pub hostname: Arc<Topic<String>>,
}

impl Hostname {
    #[cfg(feature = "demo_mode")]
    pub fn new<C>(bb: &mut BrokerBuilder, _conn: C) -> Self {
        Self {
            hostname: bb.topic_ro("/v1/tac/network/hostname", Some("lxatac".into())),
        }
    }

    #[cfg(not(feature = "demo_mode"))]
    pub fn new(bb: &mut BrokerBuilder, conn: &Arc<Connection>) -> Self {
        let hostname = bb.topic_ro("/v1/tac/network/hostname", None);

        let conn = conn.clone();
        let hostname_topic = hostname.clone();
        async_std::task::spawn(async move {
            let proxy = hostnamed::HostnameProxy::new(&conn).await.unwrap();

            let mut stream = proxy.receive_hostname_changed().await;

            if let Ok(h) = proxy.hostname().await {
                hostname_topic.set(h);
            }

            while let Some(v) = stream.next().await {
                if let Ok(h) = v.get().await {
                    hostname_topic.set(h);
                }
            }
        });

        Self { hostname }
    }
}
