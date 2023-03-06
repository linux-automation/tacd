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

use std::io::{Error, ErrorKind, Result};
use std::time::Duration;

use async_std::future::pending;
use async_std::task::sleep;

use crate::dut_power::TickReader;

#[cfg(any(test, feature = "demo_mode"))]
mod sd {
    use std::io::Result;

    pub const STATE_READY: () = ();
    pub const STATE_WATCHDOG: () = ();

    pub fn notify<I>(_: bool, _: I) -> Result<bool> {
        println!("Watchdog tick");
        Ok(true)
    }

    pub fn watchdog_enabled(_: bool) -> Result<u64> {
        Ok(5_000_000)
    }
}

#[cfg(not(any(test, feature = "demo_mode")))]
mod sd {
    pub use systemd::daemon::*;
}

use sd::{notify, watchdog_enabled, STATE_READY, STATE_WATCHDOG};

pub struct Watchdog {
    dut_power_tick: TickReader,
}

impl Watchdog {
    pub fn new(dut_power_tick: TickReader) -> Self {
        Self {
            dut_power_tick: dut_power_tick,
        }
    }

    /// Make sure the following things are still somewhat working:
    ///
    /// - async_std runtime - otherwise the future would not be polled
    /// - dut_pwr thread - otherwise the tick would not be incremented
    /// - adc thread - if the adc values are too old dut_pwr_thread will
    ///   not increment the tick.
    pub async fn keep_fed(mut self) -> Result<()> {
        let interval = {
            let micros = watchdog_enabled(false).unwrap_or(0);

            if micros == 0 {
                eprintln!("Watchdog not requested. Disabling");

                // Wait forever, as returning from this function terminated the program
                let () = pending().await;
            }

            Duration::from_micros(micros) / 2
        };

        notify(false, [(STATE_READY, "1")].iter())?;

        loop {
            sleep(interval).await;

            if self.dut_power_tick.is_stale() {
                eprintln!("Power Thread has stalled. Will trigger watchdog.");

                notify(false, [(STATE_WATCHDOG, "trigger")].iter())?;

                break Err(Error::new(
                    ErrorKind::TimedOut,
                    "Power Thread stalled for too long",
                ));
            }

            notify(false, [(STATE_WATCHDOG, "1")].iter())?;
        }
    }
}
