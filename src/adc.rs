use async_std::sync::Arc;
use async_std::task::{sleep, spawn};

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

const HISTORY_LENGTH: usize = 200;

#[cfg(any(test, feature = "stub_out_adc"))]
mod iio {
    mod stub;
    pub use stub::*;
}

#[cfg(not(any(test, feature = "stub_out_adc")))]
mod iio {
    mod hardware;
    pub use hardware::*;
}

pub use iio::{CalibratedChannel, IioThread};

/// Serialize an Instant as a javascript timestamp (f64 containing the number
/// of milliseconds since Unix Epoch 0).
/// Since Instants use a monotonic clock that is not actually related to the
/// system clock this is a somewhat handwavey process.
///
/// The idea is to take the current Instant (monotonic time) and System Time
/// (calender time) and calculate: now_system - (now_instant - ts_instant).
pub mod json_instant {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Instant, SystemTime};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let age = instant.elapsed();
        let age_as_sys = SystemTime::now().checked_sub(age).unwrap();
        let timestamp = age_as_sys.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let js_timestamp = 1000.0 * timestamp.as_secs_f64();
        js_timestamp.serialize(serializer)
    }

    pub fn deserialize<'a, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'a>,
    {
        let _js_timestamp = f64::deserialize(deserializer)?;
        unimplemented!();
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Measurement {
    #[serde(with = "json_instant")]
    pub ts: Instant,
    pub value: f32,
}

impl From<(Instant, f32)> for Measurement {
    fn from(m: (Instant, f32)) -> Self {
        Self {
            ts: m.0,
            value: m.1,
        }
    }
}

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
}

impl Adc {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        let iio_thread = IioThread::new();

        let adc = Self {
            usb_host_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/total/feedback/current",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            usb_host1_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host1-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/port1/feedback/current",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            usb_host2_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host2-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/port2/feedback/current",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            usb_host3_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host3-curr").unwrap(),
                topic: bb.topic(
                    "/v1/usb/host/port3/feedback/current",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            out0_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("out0-volt").unwrap(),
                topic: bb.topic(
                    "/v1/output/out_0/feedback/voltage",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            out1_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("out1-volt").unwrap(),
                topic: bb.topic(
                    "/v1/output/out_1/feedback/voltage",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            iobus_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("iobus-curr").unwrap(),
                topic: bb.topic(
                    "/v1/iobus/feedback/current",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            iobus_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("iobus-volt").unwrap(),
                topic: bb.topic(
                    "/v1/iobus/feedback/voltage",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            pwr_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("pwr-volt").unwrap(),
                topic: bb.topic(
                    "/v1/dut/feedback/voltage",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
            pwr_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("pwr-curr").unwrap(),
                topic: bb.topic(
                    "/v1/dut/feedback/current",
                    true,
                    false,
                    None,
                    HISTORY_LENGTH,
                ),
            },
        };

        let adc_clone = adc.clone();

        // Spawn an async task to transfer values from the Atomic value based
        // "fast" interface to the broker based "slow" interface.
        spawn(async move {
            loop {
                sleep(Duration::from_millis(100)).await;

                adc_clone
                    .usb_host_curr
                    .topic
                    .set(adc_clone.usb_host_curr.fast.get().into())
                    .await;
                adc_clone
                    .usb_host1_curr
                    .topic
                    .set(adc_clone.usb_host1_curr.fast.get().into())
                    .await;
                adc_clone
                    .usb_host2_curr
                    .topic
                    .set(adc_clone.usb_host2_curr.fast.get().into())
                    .await;
                adc_clone
                    .usb_host3_curr
                    .topic
                    .set(adc_clone.usb_host3_curr.fast.get().into())
                    .await;
                adc_clone
                    .out0_volt
                    .topic
                    .set(adc_clone.out0_volt.fast.get().into())
                    .await;
                adc_clone
                    .out1_volt
                    .topic
                    .set(adc_clone.out1_volt.fast.get().into())
                    .await;
                adc_clone
                    .iobus_curr
                    .topic
                    .set(adc_clone.iobus_curr.fast.get().into())
                    .await;
                adc_clone
                    .iobus_volt
                    .topic
                    .set(adc_clone.iobus_volt.fast.get().into())
                    .await;
                adc_clone
                    .pwr_volt
                    .topic
                    .set(adc_clone.pwr_volt.fast.get().into())
                    .await;
                adc_clone
                    .pwr_curr
                    .topic
                    .set(adc_clone.pwr_curr.fast.get().into())
                    .await;
            }
        });

        adc
    }
}
