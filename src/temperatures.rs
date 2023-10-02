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

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;
use async_std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};
use crate::measurement::Measurement;
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
mod hw {
    use anyhow::Result;

    pub(super) trait SysClass {
        fn input(&self) -> Result<u32>;
    }

    pub(super) struct HwMon;
    pub(super) struct TempDecoy;

    impl SysClass for TempDecoy {
        fn input(&self) -> Result<u32> {
            Ok(30_000)
        }
    }

    impl HwMon {
        pub(super) fn new(_: &'static str) -> Result<Self> {
            Ok(Self)
        }

        pub(super) fn temp(&self, _: u64) -> Result<TempDecoy> {
            Ok(TempDecoy)
        }
    }
}

#[cfg(not(feature = "demo_mode"))]
mod hw {
    pub(super) use sysfs_class::*;
}

use hw::{HwMon, SysClass};

const UPDATE_INTERVAL: Duration = Duration::from_millis(500);
const TEMPERATURE_SOC_CRITICAL: f32 = 90.0;
const TEMPERATURE_SOC_HIGH: f32 = 70.0;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum Warning {
    Okay,
    SocHigh,
    SocCritical,
}

impl Warning {
    fn from_temperatures(soc: f32) -> Self {
        if soc > TEMPERATURE_SOC_CRITICAL {
            Self::SocCritical
        } else if soc > TEMPERATURE_SOC_HIGH {
            Self::SocHigh
        } else {
            Self::Okay
        }
    }
}

pub struct Temperatures {
    pub soc_temperature: Arc<Topic<Measurement>>,
    pub warning: Arc<Topic<Warning>>,
    run: Option<Arc<AtomicBool>>,
}

impl Temperatures {
    pub fn new(bb: &mut BrokerBuilder, wtb: &mut WatchedTasksBuilder) -> Result<Self> {
        let run = Arc::new(AtomicBool::new(true));
        let soc_temperature = bb.topic_ro("/v1/tac/temperatures/soc", None);
        let warning = bb.topic_ro("/v1/tac/temperatures/warning", None);

        let run_thread = run.clone();
        let soc_temperature_thread = soc_temperature.clone();
        let warning_thread = warning.clone();

        wtb.spawn_thread("temperature-update", move || {
            while run_thread.load(Ordering::Relaxed) {
                let val = HwMon::new("hwmon0")?.temp(1)?.input()?;

                let val = val as f32 / 1000.0;

                // Provide a topic that only provides "is overheating"/"is okay"
                // updates and not the 2Hz temperature feed.
                // Subscribing to this topic is cheaper w.r.t. cpu/network use.
                let warning = Warning::from_temperatures(val);
                warning_thread.set_if_changed(warning);

                let meas = Measurement::now(val);
                soc_temperature_thread.set(meas);

                sleep(UPDATE_INTERVAL);
            }

            Ok(())
        })?;

        Ok(Self {
            soc_temperature,
            warning,
            run: Some(run),
        })
    }
}

impl Drop for Temperatures {
    fn drop(&mut self) {
        self.run.take().unwrap().store(false, Ordering::Relaxed);
    }
}
