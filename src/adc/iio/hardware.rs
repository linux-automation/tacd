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
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use async_std::channel::bounded;
use async_std::sync::Arc;

use industrial_io::{Buffer, Channel};

use log::{debug, error, warn};
use thread_priority::*;

use crate::measurement::{Measurement, Timestamp};
use crate::system::HardwareGeneration;
use crate::watched_tasks::WatchedTasksBuilder;

mod channels;

use channels::{ChannelDesc, Channels};

const TRIGGER_HR_PWR_DIR: &str = "/sys/kernel/config/iio/triggers/hrtimer/tacd-pwr";

// Timestamps are stored in a 64 Bit atomic variable containing the
// time in nanoseconds passed since the tacd was started.
// To reach u64::MAX the tacd would need to run for 2^64ns which is
// about 584 years.
const TIMESTAMP_ERROR: u64 = u64::MAX;

#[derive(Debug)]
pub enum AdcReadError {
    Again,
    MismatchedChannels,
    AquisitionError,
    TimeStampError,
}

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
    ) -> Result<[Measurement; N], AdcReadError> {
        let ts_before = self.iio_thread.timestamp.load(Ordering::Acquire);

        let mut values_raw = [0; N];
        for (d, ch) in values_raw.iter_mut().zip(channels.iter()) {
            // Can only get time-aligned values for channels of the same ADC
            if !Arc::ptr_eq(&self.iio_thread, &ch.iio_thread) {
                return Err(AdcReadError::MismatchedChannels);
            }

            *d = self.iio_thread.values[ch.index].load(Ordering::Relaxed);
        }

        let ts_after = self.iio_thread.timestamp.load(Ordering::Acquire);

        if ts_before == TIMESTAMP_ERROR || ts_after == TIMESTAMP_ERROR {
            return Err(AdcReadError::AquisitionError);
        }

        if ts_before == ts_after {
            let ts = self
                .iio_thread
                .ref_instant
                .checked_add(Duration::from_nanos(ts_before))
                .ok_or(AdcReadError::TimeStampError)?;
            let ts = Timestamp::new(ts);

            let mut values = [Measurement { ts, value: 0.0 }; N];
            for i in 0..N {
                values[i].value = channels[i].calibration.apply(values_raw[i] as f32);
            }

            Ok(values)
        } else {
            Err(AdcReadError::Again)
        }
    }

    /// Get the value of the channel, or None if the timestamp changed while
    /// reading the value (which should be extremely rare)
    pub fn try_get(&self) -> Result<Measurement, AdcReadError> {
        self.try_get_multiple([self]).map(|res| res[0])
    }

    // Get the current value of the channel
    pub fn get(&self) -> Result<Measurement, AdcReadError> {
        loop {
            match self.try_get() {
                Err(AdcReadError::Again) => {}
                res => break res,
            }
        }
    }
}

pub struct IioThread {
    ref_instant: Instant,
    timestamp: AtomicU64,
    values: Vec<AtomicU16>,
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

        let channels: Result<Vec<Channel>> = channel_descs
            .iter()
            .map(|ChannelDesc { kernel_name, .. }| {
                let ch = adc
                    .find_channel(kernel_name, false)
                    .ok_or_else(|| anyhow!("Failed to open iio channel {}", kernel_name));

                if let Ok(ch) = ch.as_ref() {
                    ch.enable();
                }

                ch
            })
            .collect();

        let channels = channels?;

        let trig = ctx
            .find_device(trigger_name)
            .ok_or(anyhow!("Could not find IIO trigger: {}", trigger_name))?;

        trig.attr_write_int("sampling_frequency", sample_rate)?;

        adc.set_trigger(&trig)?;
        ctx.set_timeout_ms(1000)?;

        let buf = adc.create_buffer(buffer_len, false)?;

        let prio = ThreadPriorityValue::try_from(10).map_err(|e| {
            anyhow!("Failed to set thread priority to 10 as you OS does not support it: {e:?}")
        })?;

        set_thread_priority_and_policy(
            thread_native_id(),
            ThreadPriority::Crossplatform(prio),
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
        )
        .map_err(|e| anyhow!("Failed to set realtime thread priority: {e:?}"))?;

        Ok((channels, buf))
    }

    async fn new(
        wtb: &mut WatchedTasksBuilder,
        thread_name: &'static str,
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
        let (thread_tx, thread_rx) = bounded(1);

        // Spawn a high priority thread that updates the atomic values in `thread`.
        wtb.spawn_thread(thread_name, move || {
            let (channels, mut buf) = Self::adc_setup(
                adc_name,
                trigger_name,
                sample_rate,
                channel_descs,
                buffer_len,
            )?;

            let thread = Arc::new(Self {
                ref_instant: Instant::now(),
                timestamp: AtomicU64::new(TIMESTAMP_ERROR),
                values: channels.iter().map(|_| AtomicU16::new(0)).collect(),
                channel_descs,
            });

            let thread_weak = Arc::downgrade(&thread);
            let mut signal_ready = Some((thread, thread_tx));

            // Stop running as soon as the last reference to this Arc<IioThread>
            // is dropped (e.g. the weak reference can no longer be upgraded).
            while let Some(thread) = thread_weak.upgrade() {
                if let Err(e) = buf.refill() {
                    thread.timestamp.store(TIMESTAMP_ERROR, Ordering::Relaxed);

                    error!("Failed to refill {} ADC buffer: {}", adc_name, e);

                    Err(e)?;
                }

                let values = channels.iter().map(|ch| {
                    let buf_sum: u32 = buf.channel_iter::<u16>(ch).map(|v| v as u32).sum();
                    (buf_sum / (buf.capacity() as u32)) as u16
                });

                for (d, s) in thread.values.iter().zip(values) {
                    d.store(s, Ordering::Relaxed)
                }

                // These should only fail if
                // a) The monotonic time started running backward
                // b) The tacd has been running for more than 2**64ns (584 years).
                let ts: u64 = Instant::now()
                    .checked_duration_since(thread.ref_instant)
                    .and_then(|d| d.as_nanos().try_into().ok())
                    .unwrap_or(TIMESTAMP_ERROR);

                thread.timestamp.store(ts, Ordering::Release);

                // Now that we know that the ADC actually works and we have
                // initial values: return a handle to it.
                if let Some((content, tx)) = signal_ready.take() {
                    // Can not fail in practice as the queue is only .take()n
                    // once and thus known to be empty.
                    tx.try_send(content)?;
                }
            }

            Ok(())
        })?;

        Ok(thread_rx.recv().await?)
    }

    pub async fn new_stm32(
        wtb: &mut WatchedTasksBuilder,
        hardware_generation: HardwareGeneration,
    ) -> Result<Arc<Self>> {
        let channels = hardware_generation.channels_stm32();

        Self::new(
            wtb,
            "adc-stm32",
            "48003000.adc:adc@0",
            "tim4_trgo",
            80,
            channels,
            4,
        )
        .await
    }

    pub async fn new_powerboard(
        wtb: &mut WatchedTasksBuilder,
        hardware_generation: HardwareGeneration,
    ) -> Result<Arc<Self>> {
        let hr_trigger_path = Path::new(TRIGGER_HR_PWR_DIR);

        if !hr_trigger_path.is_dir() {
            create_dir(hr_trigger_path)?;
        }

        let channels = hardware_generation.channels_pwr();

        Self::new(
            wtb,
            "adc-powerboard",
            "lmp92064",
            "tacd-pwr",
            20,
            channels,
            1,
        )
        .await
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
