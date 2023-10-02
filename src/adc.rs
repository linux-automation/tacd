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

use anyhow::Result;
use async_std::sync::Arc;
use async_std::task::sleep;

use crate::broker::{BrokerBuilder, Topic};
use crate::measurement::{Measurement, Timestamp};
use crate::watched_tasks::WatchedTasksBuilder;

const HISTORY_LENGTH: usize = 200;
const SLOW_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(test)]
mod iio {
    mod test;
    pub use test::*;
}

#[cfg(feature = "demo_mode")]
mod iio {
    mod demo_mode;
    pub use demo_mode::*;
}

#[cfg(not(any(test, feature = "demo_mode")))]
mod iio {
    mod hardware;
    pub use hardware::*;
}

pub use iio::{CalibratedChannel, IioThread};

/// A reference to an ADC channel.
///
/// The channel can be used in two different ways:
///
/// * The `fast` way uses Atomic values to provide lockless and constant
///   time access to the most recent ADC value.
/// * The `topic` way uses the tacd broker system and allow you to subscribe
///   to a stream of new values.
#[derive(Clone)]
pub struct AdcChannel {
    pub fast: CalibratedChannel,
    pub topic: Arc<Topic<Measurement>>,
}

#[derive(Clone)]
pub struct Adc {
    pub usb_host_curr: AdcChannel,
    pub usb_host1_curr: AdcChannel,
    pub usb_host2_curr: AdcChannel,
    pub usb_host3_curr: AdcChannel,
    pub out0_volt: AdcChannel,
    pub out1_volt: AdcChannel,
    pub iobus_curr: AdcChannel,
    pub iobus_volt: AdcChannel,
    pub pwr_volt: AdcChannel,
    pub pwr_curr: AdcChannel,
    pub time: Arc<Topic<Timestamp>>,
}

impl Adc {
    pub async fn new(bb: &mut BrokerBuilder, wtb: &mut WatchedTasksBuilder) -> Result<Self> {
        let stm32_thread = IioThread::new_stm32(wtb).await?;
        let powerboard_thread = IioThread::new_powerboard(wtb).await?;

        let adc = Self {
            usb_host_curr: AdcChannel {
                fast: stm32_thread.clone().get_channel("usb-host-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/total/feedback/current",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            usb_host1_curr: AdcChannel {
                fast: stm32_thread.clone().get_channel("usb-host1-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/port1/feedback/current",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            usb_host2_curr: AdcChannel {
                fast: stm32_thread.clone().get_channel("usb-host2-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/port2/feedback/current",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            usb_host3_curr: AdcChannel {
                fast: stm32_thread.clone().get_channel("usb-host3-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/port3/feedback/current",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            out0_volt: AdcChannel {
                fast: stm32_thread.clone().get_channel("out0-volt").unwrap(),
                topic: bb.topic(
                    "/v1/output/out_0/feedback/voltage",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            out1_volt: AdcChannel {
                fast: stm32_thread.clone().get_channel("out1-volt").unwrap(),
                topic: bb.topic(
                    "/v1/output/out_1/feedback/voltage",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            iobus_curr: AdcChannel {
                fast: stm32_thread.clone().get_channel("iobus-curr").unwrap(),
                topic: bb.topic(
                    "/v1/iobus/feedback/current",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            iobus_volt: AdcChannel {
                fast: stm32_thread.clone().get_channel("iobus-volt").unwrap(),
                topic: bb.topic(
                    "/v1/iobus/feedback/voltage",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            pwr_volt: AdcChannel {
                fast: powerboard_thread.clone().get_channel("pwr-volt").unwrap(),
                topic: bb.topic(
                    "/v1/dut/feedback/voltage",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            pwr_curr: AdcChannel {
                fast: powerboard_thread.get_channel("pwr-curr").unwrap(),
                topic: bb.topic(
                    "/v1/dut/feedback/current",
                    true,
                    false,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            time: bb.topic_ro("/v1/tac/time/now", None),
        };

        let channels = [
            adc.usb_host_curr.clone(),
            adc.usb_host1_curr.clone(),
            adc.usb_host2_curr.clone(),
            adc.usb_host3_curr.clone(),
            adc.out0_volt.clone(),
            adc.out1_volt.clone(),
            adc.iobus_curr.clone(),
            adc.iobus_volt.clone(),
            adc.pwr_volt.clone(),
            adc.pwr_curr.clone(),
        ];

        let time = adc.time.clone();

        // Spawn an async task to transfer values from the Atomic value based
        // "fast" interface to the broker based "slow" interface.
        wtb.spawn_task("adc-update", async move {
            loop {
                sleep(SLOW_INTERVAL).await;

                for channel in &channels {
                    if let Ok(val) = channel.fast.get() {
                        // The adc channel topic should likely be wrapped in a Result
                        // or otherwise be able to contain an error state.
                        channel.topic.set(val)
                    }
                }

                time.set(Timestamp::now());
            }
        });

        Ok(adc)
    }
}
