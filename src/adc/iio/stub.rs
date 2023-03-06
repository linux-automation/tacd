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

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use async_std::sync::Arc;

const NO_TRANSIENT: u32 = u32::MAX;

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
    transient: Arc<AtomicU32>,
}

impl CalibratedChannel {
    fn new() -> Self {
        Self {
            val: Arc::new(AtomicU32::new(0)),
            stall: Arc::new(AtomicBool::new(false)),
            transient: Arc::new(AtomicU32::new(NO_TRANSIENT)),
        }
    }

    pub fn try_get_multiple<const N: usize>(
        &self,
        channels: [&Self; N],
    ) -> Option<(Instant, [f32; N])> {
        let mut results = [0.0; N];

        for i in 0..N {
            // If a transient is scheduled (channels[i].transient != NO_TRANSIENT)
            // output it exactly once. Otherwise output the normal value.
            let transient_u32 = channels[i].transient.swap(NO_TRANSIENT, Ordering::Relaxed);
            let val_u32 = match transient_u32 {
                NO_TRANSIENT => channels[i].val.load(Ordering::Relaxed),
                transient => transient,
            };

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

    #[cfg(test)]
    pub fn set(&self, val: f32) {
        self.val.store(val.to_bits(), Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn stall(&self, state: bool) {
        self.stall.store(state, Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn transient(&self, val: f32) {
        self.transient.store(val.to_bits(), Ordering::Relaxed)
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
