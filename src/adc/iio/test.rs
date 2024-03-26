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
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_std::sync::Arc;

use crate::measurement::{Measurement, Timestamp};

const NO_TRANSIENT: u32 = u32::MAX;

const CHANNELS_STM32: &[&str] = &[
    "usb-host-curr",
    "usb-host1-curr",
    "usb-host2-curr",
    "usb-host3-curr",
    "out0-volt",
    "out1-volt",
    "iobus-curr",
    "iobus-volt",
];

const CHANNELS_PWR: &[&str] = &["pwr-volt", "pwr-curr"];

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
    ) -> Result<[Measurement; N]> {
        let mut ts = Timestamp::now();

        if self.stall.load(Ordering::Relaxed) {
            *ts -= Duration::from_millis(500)
        }

        let mut results = [Measurement { ts, value: 0.0 }; N];

        for i in 0..N {
            // If a transient is scheduled (channels[i].transient != NO_TRANSIENT)
            // output it exactly once. Otherwise output the normal value.
            let transient_u32 = channels[i].transient.swap(NO_TRANSIENT, Ordering::Relaxed);
            let val_u32 = match transient_u32 {
                NO_TRANSIENT => channels[i].val.load(Ordering::Relaxed),
                transient => transient,
            };

            results[i].value = f32::from_bits(val_u32);
        }

        Ok(results)
    }

    pub fn try_get(&self) -> Result<Measurement> {
        self.try_get_multiple([self]).map(|res| res[0])
    }

    pub fn get(&self) -> Result<Measurement> {
        self.try_get()
    }

    pub fn set(&self, val: f32) {
        self.val.store(val.to_bits(), Ordering::Relaxed)
    }

    pub fn stall(&self, state: bool) {
        self.stall.store(state, Ordering::Relaxed)
    }

    pub fn transient(&self, val: f32) {
        self.transient.store(val.to_bits(), Ordering::Relaxed)
    }
}

pub struct IioThread {
    channels: Vec<(&'static str, CalibratedChannel)>,
}

impl IioThread {
    pub async fn new_stm32<W, G>(_wtb: &W, _hardware_generation: G) -> Result<Arc<Self>> {
        let mut channels = Vec::new();

        for name in CHANNELS_STM32 {
            channels.push((*name, CalibratedChannel::new()))
        }

        Ok(Arc::new(Self { channels }))
    }

    pub async fn new_powerboard<W>(_wtb: &W) -> Result<Arc<Self>> {
        let mut channels = Vec::new();

        for name in CHANNELS_PWR {
            channels.push((*name, CalibratedChannel::new()))
        }

        Ok(Arc::new(Self { channels }))
    }

    pub fn get_channel(self: Arc<Self>, ch_name: &str) -> Result<CalibratedChannel> {
        self.channels
            .iter()
            .find(|(name, _)| *name == ch_name)
            .ok_or(anyhow!("Could not get adc channel {}", ch_name))
            .map(|(_, chan)| chan.clone())
    }
}
