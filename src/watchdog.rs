use std::io::{Error, ErrorKind, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_std::future::pending;
use async_std::sync::Arc;
use async_std::task::sleep;

use systemd::daemon::{notify, watchdog_enabled, STATE_READY, STATE_WATCHDOG};

pub struct Watchdog {
    dut_power_tick: Arc<AtomicU32>,
}

impl Watchdog {
    pub fn new(dut_power_tick: &Arc<AtomicU32>) -> Self {
        Self {
            dut_power_tick: dut_power_tick.clone(),
        }
    }

    /// Make sure the following things are still somewhat working:
    ///
    /// - async_std runtime - otherwise the future would not be polled
    /// - dut_pwr thread - otherwise the tick would not be incremented
    /// - adc thread - if the adc values are too old dut_pwr_thread will
    ///   not increment the tick.
    pub async fn keep_fed(self) -> Result<()> {
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

        let mut prev = self.dut_power_tick.load(Ordering::Relaxed);

        loop {
            sleep(interval).await;

            let curr = self.dut_power_tick.load(Ordering::Relaxed);

            if prev == curr {
                eprintln!("Power Thread has stalled. Will trigger watchdog.");

                notify(false, [(STATE_WATCHDOG, "trigger")].iter())?;

                break Err(Error::new(
                    ErrorKind::TimedOut,
                    "Power Thread stalled for too long",
                ));
            }

            notify(false, [(STATE_WATCHDOG, "1")].iter())?;
            prev = curr;
        }
    }
}
