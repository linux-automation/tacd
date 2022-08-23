use anyhow::{anyhow, Context, Result};

use std::convert::{TryFrom, TryInto};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use async_std::sync::Arc;

use industrial_io::Channel;

use log::debug;
use thread_priority::*;

// Hard coded list of channels using the internal STM32MP1 ADC.
// Consists of the IIO channel name, the location of the calibration data
// in the device tree and an internal name for the channel.
const CHANNELS_STM32: &[(&str, &str, &str)] = &[
    (
        "voltage13",
        "baseboard-factory-data/usb-host-curr",
        "usb-host-curr",
    ),
    (
        "voltage15",
        "baseboard-factory-data/usb-host1-curr",
        "usb-host1-curr",
    ),
    (
        "voltage0",
        "baseboard-factory-data/usb-host2-curr",
        "usb-host2-curr",
    ),
    (
        "voltage1",
        "baseboard-factory-data/usb-host3-curr",
        "usb-host3-curr",
    ),
    ("voltage2", "baseboard-factory-data/out0-volt", "out0-volt"),
    ("voltage10", "baseboard-factory-data/out1-volt", "out1-volt"),
    (
        "voltage5",
        "baseboard-factory-data/iobus-curr",
        "iobus-curr",
    ),
    (
        "voltage9",
        "baseboard-factory-data/iobus-volt",
        "iobus-volt",
    ),
];

// The same as for the STM32MP1 channels but for the discrete ADC on the power
// board.
const CHANNELS_PWR: &[(&str, &str, &str)] = &[
    ("voltage", "powerboard-factory-data/pwr-volt", "pwr-volt"),
    ("current", "powerboard-factory-data/pwr-curr", "pwr-curr"),
];

#[derive(Clone, Copy)]
struct Calibration {
    scale: f32,
    offset: f32,
}

impl Calibration {
    /// Load ADC-Calibration data from `path`
    ///
    /// The `path` should most likely point to somewhere in the devicetree
    /// choosen parameters.
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
    /// constant.
    ///
    /// As only a tiny fraction of overall runtime is spent updating the values
    /// and timestamps it should be safe to just call this in a loop until it
    /// succeeds.
    pub fn try_get_multiple<const N: usize>(
        &self,
        channels: [&Self; N],
    ) -> Option<(Instant, [f32; N])> {
        let ts_before = self.iio_thread.timestamp.load(Ordering::Acquire);

        // TODO: should there be a fence() here?

        let mut values_raw = [0; N];
        for (d, ch) in values_raw.iter_mut().zip(channels.iter()) {
            assert!(
                Arc::ptr_eq(&self.iio_thread, &ch.iio_thread),
                "Can only get synchronized adc values for the same thread"
            );
            *d = self.iio_thread.values[ch.index].load(Ordering::Relaxed);
        }

        // TODO: should there be a fence() here?

        let ts_after = self.iio_thread.timestamp.load(Ordering::Acquire);

        if ts_before == ts_after {
            let ts = self
                .iio_thread
                .ref_instant
                .checked_add(Duration::from_nanos(ts_before))
                .unwrap();

            let mut values = [0.0; N];
            for i in 0..N {
                values[i] = channels[i].calibration.apply(values_raw[i] as f32);
            }

            Some((ts, values))
        } else {
            None
        }
    }

    /// Get the value of the channel, or None if the timestamp changed while
    /// reading the value (which should be extremely rare)
    pub fn try_get(&self) -> Option<(Instant, f32)> {
        self.try_get_multiple([self]).map(|(ts, [val])| (ts, val))
    }

    // Get the current value of the channel
    pub fn get(&self) -> (Instant, f32) {
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
    values: [AtomicU16; 10],
    join: Mutex<Option<JoinHandle<()>>>,
}

impl IioThread {
    pub fn new() -> Arc<Self> {
        let thread = Arc::new(Self {
            ref_instant: Instant::now(),
            timestamp: AtomicU64::new(0),
            values: [
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
                AtomicU16::new(0),
            ],
            join: Mutex::new(None),
        });

        let thread_weak = Arc::downgrade(&thread);

        // Spawn a high priority thread that updates the atomic values in `thread`.
        let join = thread::Builder::new()
            .name("tacd iio".into())
            .spawn(move || {
                let mut ctx = industrial_io::Context::new().unwrap();

                debug!("IIO devices:");
                for dev in ctx.devices() {
                    debug!("  * {}", &dev.name().unwrap_or_default());
                }

                let mut stm32_adc = ctx.find_device("48003000.adc:adc@0").unwrap();
                let pwr_adc = ctx.find_device("lmp92064").unwrap();

                // FIXME: This should really be done via some attr_write call or similar
                let buffer_enable_path = format!(
                    "/sys/bus/iio/devices/{}/buffer/enable",
                    stm32_adc.id().unwrap()
                );
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(&buffer_enable_path)
                    .unwrap()
                    .write_all(b"0\n")
                    .unwrap();

                let stm32_channels: Vec<Channel> = CHANNELS_STM32
                    .iter()
                    .map(|(iio_name, _, _)| {
                        let mut ch = stm32_adc
                            .find_channel(iio_name, false)
                            .expect(&format!("Failed to open iio channel {iio_name}"));

                        ch.enable();
                        ch
                    })
                    .collect();

                let pwr_channels: Vec<Channel> = CHANNELS_PWR
                    .iter()
                    .map(|(iio_name, _, _)| {
                        pwr_adc
                            .find_channel(iio_name, false)
                            .expect(&format!("Failed to open iio channel {iio_name}"))
                    })
                    .collect();

                let trig = ctx.find_device("tim4_trgo").unwrap();
                trig.attr_write_int("sampling_frequency", 1024).unwrap();

                stm32_adc.set_trigger(&trig).unwrap();
                stm32_adc.set_num_kernel_buffers(32).unwrap();
                ctx.set_timeout_ms(1000).unwrap();

                let mut stm32_buf = stm32_adc.create_buffer(128, false).unwrap();

                set_thread_priority_and_policy(
                    thread_native_id(),
                    ThreadPriority::Crossplatform(ThreadPriorityValue::try_from(10).unwrap()),
                    ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
                )
                .unwrap();

                // Stop running as soon as the last refernce to this Arc<IioThread>
                // is dropped (e.g. the weak reference can no longer be upgraded).
                while let Some(thread) = thread_weak.upgrade() {
                    // Use the buffer interface to get STM32 ADC values at a high
                    // sampling rate to perform averaging in software.
                    stm32_buf.refill().unwrap();

                    let stm32_values = stm32_channels.iter().map(|ch| {
                        let buf_sum: u32 =
                            stm32_buf.channel_iter::<u16>(ch).map(|v| v as u32).sum();
                        (buf_sum / (stm32_buf.capacity() as u32)) as u16
                    });

                    // Use the sysfs based interface to get the values from the
                    // power board ADC at a slow sampling rate.
                    let pwr_values = pwr_channels
                        .iter()
                        .map(|ch| ch.attr_read_int("raw").unwrap() as u16);

                    // The power board values are located after the stm32 values
                    // in the thread.values array.
                    let values = stm32_values.chain(pwr_values);

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
                }
            })
            .unwrap();

        *thread.join.lock().unwrap() = Some(join);

        thread
    }

    /// Use the channel names defined at the top of the file to get a reference
    /// to a channel
    pub fn get_channel(self: Arc<Self>, ch_name: &str) -> Result<CalibratedChannel> {
        CHANNELS_STM32
            .iter()
            .chain(CHANNELS_PWR)
            .enumerate()
            .filter(|(_, (_, _, name))| name == &ch_name)
            .next()
            .ok_or(anyhow!("Could not get adc channel {}", ch_name))
            .and_then(|(idx, (_, calib_name, _))| {
                CalibratedChannel::from_name(self.clone(), idx, calib_name)
            })
    }
}
