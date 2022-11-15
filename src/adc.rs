use async_std::sync::Arc;
use async_std::task::{sleep, spawn};

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::broker::{BrokerBuilder, Topic};

mod iio;

pub use iio::{CalibratedChannel, IioThread};

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
                topic: bb.topic_ro("/v1/usb/host/total/feedback/current"),
            },
            usb_host1_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host1-curr").unwrap(),
                topic: bb.topic_ro("/v1/usb/host/port1/feedback/current"),
            },
            usb_host2_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host2-curr").unwrap(),
                topic: bb.topic_ro("/v1/usb/host/port2/feedback/current"),
            },
            usb_host3_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("usb-host3-curr").unwrap(),
                topic: bb.topic_ro("/v1/usb/host/port3/feedback/current"),
            },
            out0_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("out0-volt").unwrap(),
                topic: bb.topic_ro("/v1/output/out_0/feedback/voltage"),
            },
            out1_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("out1-volt").unwrap(),
                topic: bb.topic_ro("/v1/output/out_1/feedback/voltage"),
            },
            iobus_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("iobus-curr").unwrap(),
                topic: bb.topic_ro("/v1/iobus/feedback/current"),
            },
            iobus_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("iobus-volt").unwrap(),
                topic: bb.topic_ro("/v1/iobus/feedback/voltage"),
            },
            pwr_volt: AdcChannel {
                fast: iio_thread.clone().get_channel("pwr-volt").unwrap(),
                topic: bb.topic_ro("/v1/power/dut/feedback/voltage"),
            },
            pwr_curr: AdcChannel {
                fast: iio_thread.clone().get_channel("pwr-curr").unwrap(),
                topic: bb.topic_ro("/v1/power/dut/feedback/current"),
            },
        };

        let adc_clone = adc.clone();

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
