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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;

use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
mod reg {
    use std::io::Result;

    use async_std::task::block_on;

    use crate::adc::IioThread;

    pub fn regulator_set(name: &str, state: bool) -> Result<()> {
        if name == "output_iobus_12v" {
            let iio_thread = block_on(IioThread::new_stm32(&(), ())).unwrap();

            iio_thread
                .clone()
                .get_channel("iobus-curr")
                .unwrap()
                .set(state);
            iio_thread.get_channel("iobus-volt").unwrap().set(state);
        }

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
    #[allow(dead_code)]
    pub uart_pwr_en: Arc<Topic<bool>>,
}

fn handle_regulator(
    bb: &mut BrokerBuilder,
    wtb: &mut WatchedTasksBuilder,
    path: &str,
    regulator_name: &'static str,
    initial: bool,
) -> Result<Arc<Topic<bool>>> {
    let topic = bb.topic_rw(path, Some(initial));
    let (mut src, _) = topic.clone().subscribe_unbounded();

    wtb.spawn_task(format!("regulator-{regulator_name}-action"), async move {
        while let Some(ev) = src.next().await {
            regulator_set(regulator_name, ev).unwrap();
        }

        Ok(())
    })?;

    Ok(topic)
}

impl Regulators {
    pub fn new(bb: &mut BrokerBuilder, wtb: &mut WatchedTasksBuilder) -> Result<Self> {
        Ok(Self {
            iobus_pwr_en: handle_regulator(bb, wtb, "/v1/iobus/powered", "output-iobus-12v", true)?,
            uart_pwr_en: handle_regulator(bb, wtb, "/v1/uart/powered", "output-vuart", true)?,
        })
    }
}
