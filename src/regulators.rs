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

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;

use crate::broker::{BrokerBuilder, Topic};

#[cfg(feature = "demo_mode")]
mod reg {
    use std::io::Result;

    pub fn regulator_set(name: &str, state: bool) -> Result<()> {
        let state = if state { "enabled" } else { "disabled" };
        println!("Regulator: would set {name} to {state} but don't feel like it");

        Ok(())
    }
}

#[cfg(not(feature = "demo_mode"))]
mod reg {
    use std::fs::write;
    use std::io::Result;
    use std::path::Path;

    pub fn regulator_set(name: &str, state: bool) -> Result<()> {
        let path = Path::new("/sys/devices/platform").join(name).join("state");
        let state = if state { "enabled" } else { "disabled" };

        write(path, state)
    }
}

use reg::regulator_set;

pub struct Regulators {
    pub iobus_pwr_en: Arc<Topic<bool>>,
    pub uart_pwr_en: Arc<Topic<bool>>,
}

fn handle_regulator(
    bb: &mut BrokerBuilder,
    path: &str,
    regulator_name: &'static str,
    initial: bool,
) -> Arc<Topic<bool>> {
    let topic = bb.topic_rw(path, Some(initial));
    let topic_task = topic.clone();

    spawn(async move {
        let (mut src, _) = topic_task.subscribe_unbounded();

        while let Some(ev) = src.next().await {
            regulator_set(regulator_name, ev).unwrap();
        }
    });

    topic
}

impl Regulators {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        Self {
            iobus_pwr_en: handle_regulator(bb, "/v1/iobus/powered", "output_iobus_12v", true),
            uart_pwr_en: handle_regulator(bb, "/v1/uart/powered", "output_vuart", true),
        }
    }
}
