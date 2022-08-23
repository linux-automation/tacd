use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use async_std::sync::Arc;

const CHANNELS: &[&str] = &[
    "usb-host-curr",
    "usb-host1-curr",
    "usb-host2-curr",
    "usb-host3-curr",
    "out0-volt",
    "out1-volt",
    "iobus-curr",
    "iobus-volt",
    "pwr-volt",
    "pwr-curr",
];

#[derive(Clone)]
pub struct CalibratedChannel {
    val: Arc<AtomicU32>,
    stall: Arc<AtomicBool>,
}

impl CalibratedChannel {
    fn new() -> Self {
        Self {
            val: Arc::new(AtomicU32::new(0)),
            stall: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn try_get_multiple<const N: usize>(
        &self,
        channels: [&Self; N],
    ) -> Option<(Instant, [f32; N])> {
        let mut results = [0.0; N];

        for i in 0..N {
            let val_u32 = channels[i].val.load(Ordering::Relaxed);
            results[i] = f32::from_bits(val_u32);
        }

        let mut ts = Instant::now();

        if self.stall.load(Ordering::Relaxed) {
            ts -= Duration::from_millis(500)
        }

        Some((ts, results))
    }

    pub fn try_get(&self) -> Option<(Instant, f32)> {
        self.try_get_multiple([self]).map(|(ts, [val])| (ts, val))
    }

    pub fn get(&self) -> (Instant, f32) {
        loop {
            if let Some(r) = self.try_get() {
                break r;
            }
        }
    }

    pub fn set(&self, val: f32) {
        self.val.store(val.to_bits(), Ordering::Relaxed)
    }

    pub fn stall(&self, state: bool) {
        self.stall.store(state, Ordering::Relaxed)
    }
}

pub struct IioThread {
    channels: Vec<(&'static str, CalibratedChannel)>,
}

impl IioThread {
    pub fn new() -> Arc<Self> {
        let mut channels = Vec::new();

        for name in CHANNELS {
            channels.push((*name, CalibratedChannel::new()))
        }

        Arc::new(Self { channels })
    }

    pub fn get_channel(self: Arc<Self>, ch_name: &str) -> Result<CalibratedChannel> {
        self.channels
            .iter()
            .find(|(name, _)| *name == ch_name)
            .ok_or(anyhow!("Could not get adc channel {}", ch_name))
            .map(|(_, chan)| chan.clone())
    }
}
