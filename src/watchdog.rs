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

use std::time::Duration;

use anyhow::{bail, Result};
use async_std::task::sleep;

use crate::dut_power::TickReader;
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(any(test, feature = "demo_mode"))]
mod sd {
    use std::io::Result;

    pub(super) const STATE_READY: () = ();
    pub(super) const STATE_WATCHDOG: () = ();

    pub(super) fn notify<I>(_: bool, _: I) -> Result<bool> {
        Ok(true)
    }

    pub(super) fn watchdog_enabled(_: bool) -> Result<u64> {
        Ok(5_000_000)
    }
}

#[cfg(not(any(test, feature = "demo_mode")))]
mod sd {
    pub(super) use systemd::daemon::*;
}

use sd::{notify, watchdog_enabled, STATE_READY, STATE_WATCHDOG};

pub struct Watchdog {
    interval: Duration,
    dut_power_tick: TickReader,
}

impl Watchdog {
    pub fn new(dut_power_tick: TickReader) -> Option<Self> {
        let micros = watchdog_enabled(false).unwrap_or(0);

        if micros != 0 {
            let interval = Duration::from_micros(micros) / 2;

            Some(Self {
                interval,
                dut_power_tick,
            })
        } else {
            log::info!("Watchdog not requested. Disabling");
            None
        }
    }

    /// Make sure the following things are still somewhat working:
    ///
    /// - async_std runtime - otherwise the future would not be polled
    /// - dut_pwr thread - otherwise the tick would not be incremented
    /// - adc thread - if the adc values are too old dut_pwr_thread will
    ///   not increment the tick.
    pub fn keep_fed(mut self, wtb: &mut WatchedTasksBuilder) -> Result<()> {
        notify(false, [(STATE_READY, "1")].iter())?;

        wtb.spawn_task("watchdog-feeder", async move {
            loop {
                sleep(self.interval).await;

                if self.dut_power_tick.is_stale() {
                    notify(false, [(STATE_WATCHDOG, "trigger")].iter())?;

                    bail!("Power Thread stalled for too long");
                }

                notify(false, [(STATE_WATCHDOG, "1")].iter())?;
            }
        })?;

        Ok(())
    }
}
