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

use std::convert::{TryFrom, TryInto};
use std::fs::create_dir;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Error, Result};
use async_std::channel::bounded;
use async_std::stream::StreamExt;
use async_std::sync::Arc;

use industrial_io::{Buffer, Channel};

use log::{debug, error, warn};
use thread_priority::*;

use crate::measurement::{Measurement, Timestamp};

struct ChannelDesc {
    kernel_name: &'static str,
    calibration_path: &'static str,
    name: &'static str,
}

// Hard coded list of channels using the internal STM32MP1 ADC.
// Consists of the IIO channel name, the location of the calibration data
// in the device tree and an internal name for the channel.
const CHANNELS_STM32: &[ChannelDesc] = &[
    ChannelDesc {
        kernel_name: "voltage13",
        calibration_path: "baseboard-factory-data/usb-host-curr",
        name: "usb-host-curr",
    },
    ChannelDesc {
        kernel_name: "voltage15",
        calibration_path: "baseboard-factory-data/usb-host1-curr",
        name: "usb-host1-curr",
    },
    ChannelDesc {
        kernel_name: "voltage0",
        calibration_path: "baseboard-factory-data/usb-host2-curr",
        name: "usb-host2-curr",
    },
    ChannelDesc {
        kernel_name: "voltage1",
        calibration_path: "baseboard-factory-data/usb-host3-curr",
        name: "usb-host3-curr",
    },
    ChannelDesc {
        kernel_name: "voltage2",
        calibration_path: "baseboard-factory-data/out0-volt",
        name: "out0-volt",
    },
    ChannelDesc {
        kernel_name: "voltage10",
        calibration_path: "baseboard-factory-data/out1-volt",
        name: "out1-volt",
    },
    ChannelDesc {
        kernel_name: "voltage5",
        calibration_path: "baseboard-factory-data/iobus-curr",
        name: "iobus-curr",
    },
    ChannelDesc {
        kernel_name: "voltage9",
        calibration_path: "baseboard-factory-data/iobus-volt",
        name: "iobus-volt",
    },
];

// The same as for the STM32MP1 channels but for the discrete ADC on the power
// board.
const CHANNELS_PWR: &[ChannelDesc] = &[
    ChannelDesc {
        kernel_name: "voltage",
        calibration_path: "powerboard-factory-data/pwr-volt",
        name: "pwr-volt",
    },
    ChannelDesc {
        kernel_name: "current",
        calibration_path: "powerboard-factory-data/pwr-curr",
        name: "pwr-curr",
    },
];

const TRIGGER_HR_PWR_DIR: &str = "/sys/kernel/config/iio/triggers/hrtimer/tacd-pwr";

const TIMESTAMP_ERROR: u64 = u64::MAX;

#[derive(Clone, Copy)]
struct Calibration {
    scale: f32,
    offset: f32,
}

impl Calibration {
    /// Load ADC-Calibration data from `path`
    ///
    /// The `path` should most likely point to somewhere in the devicetree
    /// chosen parameters.
    fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let mut fd = std::fs::File::open(path.as_ref()).with_context(|| {
            format!(
                "Failed to read adc calibration data from {}",
                path.as_ref().to_str().unwrap_or("<broken pathname>")
            )
        })?;

        let scale = {
            let mut buf = [0u8; 4];
            fd.read_exact(&mut buf)?;
            f32::from_be_bytes(buf)
        };

        let offset = {
            let mut buf = [0u8; 4];
            fd.read_exact(&mut buf)?;
            f32::from_be_bytes(buf)
        };

        Ok(Self { scale, offset })
    }

    fn from_devicetree_chosen(name: &str) -> Result<Self> {
        let path = std::path::Path::new("/sys/firmware/devicetree/base/chosen").join(name);

        Self::from_file(path)
    }

    fn apply(&self, val: f32) -> f32 {
        val * self.scale - self.offset
    }
}

#[derive(Clone)]
pub struct CalibratedChannel {
    iio_thread: Arc<IioThread>,
    index: usize,
    calibration: Calibration,
}

impl CalibratedChannel {
    /// Create a new calibrated channel using calibration data from `calibration_name`.
    /// Values will be read from the value array of `iio_thread` at index `index`.
    fn from_name(iio_thread: Arc<IioThread>, index: usize, calibration_name: &str) -> Result<Self> {
        let calibration = Calibration::from_devicetree_chosen(calibration_name)?;

        Ok(Self {
            iio_thread,
            index,
            calibration,
        })
    }

    /// Get values for multiple channels of the same `iio_thread` that were
    /// sampled at the same timestamp.
    ///
    /// Returns None if not all values could be read while the timestamp stayed
    /// constant or if no values were acquired yet.
    ///
    /// As only a tiny fraction of overall runtime is spent updating the values
    /// and timestamps it should be safe to just call this in a loop until it
    /// succeeds.
    pub fn try_get_multiple<const N: usize>(
        &self,
        channels: [&Self; N],
    ) -> Option<[Measurement; N]> {
        let ts_before = self.iio_thread.timestamp.load(Ordering::Acquire);

        let mut values_raw = [0; N];
        for (d, ch) in values_raw.iter_mut().zip(channels.iter()) {
            assert!(
                Arc::ptr_eq(&self.iio_thread, &ch.iio_thread),
                "Can only get synchronized adc values for the same thread"
            );
            *d = self.iio_thread.values[ch.index].load(Ordering::Relaxed);
        }

        let ts_after = self.iio_thread.timestamp.load(Ordering::Acquire);

        if ts_before == TIMESTAMP_ERROR || ts_after == TIMESTAMP_ERROR {
            panic!("Failed to read from ADC");
        }

        if ts_before == ts_after {
            let ts = self
                .iio_thread
                .ref_instant
                .checked_add(Duration::from_nanos(ts_before))
                .unwrap();
            let ts = Timestamp::new(ts);

            let mut values = [Measurement { ts, value: 0.0 }; N];
            for i in 0..N {
                values[i].value = channels[i].calibration.apply(values_raw[i] as f32);
            }

            Some(values)
        } else {
            None
        }
    }

    /// Get the value of the channel, or None if the timestamp changed while
    /// reading the value (which should be extremely rare)
    pub fn try_get(&self) -> Option<Measurement> {
        self.try_get_multiple([self]).map(|res| res[0])
    }

    // Get the current value of the channel
    pub fn get(&self) -> Measurement {
        loop {
            if let Some(r) = self.try_get() {
                break r;
            }
        }
    }
}

pub struct IioThread {
    ref_instant: Instant,
    timestamp: AtomicU64,
    values: Vec<AtomicU16>,
    join: Mutex<Option<JoinHandle<()>>>,
    channel_descs: &'static [ChannelDesc],
}

impl IioThread {
    fn adc_setup(
        adc_name: &str,
        trigger_name: &str,
        sample_rate: i64,
        channel_descs: &[ChannelDesc],
        buffer_len: usize,
    ) -> Result<(Vec<Channel>, Buffer)> {
        let ctx = industrial_io::Context::new()?;

        debug!("IIO devices:");
        for dev in ctx.devices() {
            debug!("  * {}", &dev.name().unwrap_or_default());
        }

        let adc = ctx
            .find_device(adc_name)
            .ok_or(anyhow!("Could not find ADC: {}", adc_name))?;

        if let Err(err) = adc.attr_write_bool("buffer/enable", false) {
            warn!("Failed to disable {} ADC buffer: {}", adc_name, err);
        }

        let channels: Vec<Channel> = channel_descs
            .iter()
            .map(|ChannelDesc { kernel_name, .. }| {
                let ch = adc
                    .find_channel(kernel_name, false)
                    .unwrap_or_else(|| panic!("Failed to open kernel channel {}", kernel_name));

                ch.enable();
                ch
            })
            .collect();

        let trig = ctx
            .find_device(trigger_name)
            .ok_or(anyhow!("Could not find IIO trigger: {}", trigger_name))?;

        trig.attr_write_int("sampling_frequency", sample_rate)?;

        adc.set_trigger(&trig)?;
        ctx.set_timeout_ms(1000)?;

        let buf = adc.create_buffer(buffer_len, false)?;

        set_thread_priority_and_policy(
            thread_native_id(),
            ThreadPriority::Crossplatform(ThreadPriorityValue::try_from(10).unwrap()),
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
        )
        .map_err(|e| anyhow!("Failed to set realtime thread priority: {e:?}"))?;

        Ok((channels, buf))
    }

    async fn new(
        thread_name: &str,
        adc_name: &'static str,
        trigger_name: &'static str,
        sample_rate: i64,
        channel_descs: &'static [ChannelDesc],
        buffer_len: usize,
    ) -> Result<Arc<Self>> {
        // Some of the adc thread setup can only happen _in_ the adc thread,
        // like setting the priority or some iio setup, as not all structs
        // are Send.
        // We do however not want to return from new() before we know that the
        // setup was sucessful.
        // This is why we create Self inside the thread and send it back
        // to the calling thread via a queue.
        let (thread_res_tx, mut thread_res_rx) = bounded(1);

        // Spawn a high priority thread that updates the atomic values in `thread`.
        let join = thread::Builder::new()
            .name(format!("tacd {thread_name} iio"))
            .spawn(move || {
                let adc_setup_res = Self::adc_setup(
                    adc_name,
                    trigger_name,
                    sample_rate,
                    channel_descs,
                    buffer_len,
                );
                let (thread, channels, mut buf) = match adc_setup_res {
                    Ok((channels, buf)) => {
                        let thread = Arc::new(Self {
                            ref_instant: Instant::now(),
                            timestamp: AtomicU64::new(TIMESTAMP_ERROR),
                            values: channels.iter().map(|_| AtomicU16::new(0)).collect(),
                            join: Mutex::new(None),
                            channel_descs,
                        });

                        (thread, channels, buf)
                    }
                    Err(e) => {
                        thread_res_tx.try_send(Err(e)).unwrap();
                        return;
                    }
                };

                let thread_weak = Arc::downgrade(&thread);
                let mut signal_ready = Some((thread, thread_res_tx));

                // Stop running as soon as the last reference to this Arc<IioThread>
                // is dropped (e.g. the weak reference can no longer be upgraded).
                while let Some(thread) = thread_weak.upgrade() {
                    if let Err(e) = buf.refill() {
                        thread.timestamp.store(TIMESTAMP_ERROR, Ordering::Relaxed);

                        error!("Failed to refill {} ADC buffer: {}", adc_name, e);

                        // If the ADC has not yet produced any values we still have the
                        // queue at hand that signals readiness to the main thread.
                        // This gives us a chance to return an Err from new().
                        // If the queue was already used just print an error instead.
                        if let Some((_, tx)) = signal_ready.take() {
                            tx.try_send(Err(Error::new(e))).unwrap();
                        }

                        break;
                    }

                    let values = channels.iter().map(|ch| {
                        let buf_sum: u32 = buf.channel_iter::<u16>(ch).map(|v| v as u32).sum();
                        (buf_sum / (buf.capacity() as u32)) as u16
                    });

                    for (d, s) in thread.values.iter().zip(values) {
                        d.store(s, Ordering::Relaxed)
                    }

                    let ts: u64 = Instant::now()
                        .checked_duration_since(thread.ref_instant)
                        .unwrap()
                        .as_nanos()
                        .try_into()
                        .unwrap();

                    thread.timestamp.store(ts, Ordering::Release);

                    // Now that we know that the ADC actually works and we have
                    // initial values: return a handle to it.
                    if let Some((content, tx)) = signal_ready.take() {
                        tx.try_send(Ok(content)).unwrap();
                    }
                }
            })?;

        let thread = thread_res_rx.next().await.unwrap()?;
        *thread.join.lock().unwrap() = Some(join);

        Ok(thread)
    }

    pub async fn new_stm32() -> Result<Arc<Self>> {
        Self::new(
            "stm32",
            "48003000.adc:adc@0",
            "tim4_trgo",
            80,
            CHANNELS_STM32,
            4,
        )
        .await
    }

    pub async fn new_powerboard() -> Result<Arc<Self>> {
        let hr_trigger_path = Path::new(TRIGGER_HR_PWR_DIR);

        if !hr_trigger_path.is_dir() {
            create_dir(hr_trigger_path).unwrap();
        }

        Self::new("powerboard", "lmp92064", "tacd-pwr", 20, CHANNELS_PWR, 1).await
    }

    /// Use the channel names defined at the top of the file to get a reference
    /// to a channel
    pub fn get_channel(self: Arc<Self>, ch_name: &str) -> Result<CalibratedChannel> {
        self.channel_descs
            .iter()
            .enumerate()
            .find(|(_, ChannelDesc { name, .. })| name == &ch_name)
            .ok_or(anyhow!("Could not get adc channel {}", ch_name))
            .and_then(
                |(
                    idx,
                    ChannelDesc {
                        calibration_path, ..
                    },
                )| {
                    CalibratedChannel::from_name(self.clone(), idx, calibration_path)
                },
            )
    }
}
