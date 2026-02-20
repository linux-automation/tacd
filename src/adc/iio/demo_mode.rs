// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{anyhow, Result};
use async_std::sync::{Arc, Mutex};
use async_std::task::block_on;
use rand::random;

use crate::measurement::{Measurement, Timestamp};

// We need to somehow get the output states from digital_io/gpio/demo_mode.rs
// to here. We could clobber the actual business code even more, or do dirty
// mutable globals stuff.
pub static DEMO_MAGIC_STM32: Mutex<Option<Arc<IioThread>>> = Mutex::new(None);
pub static DEMO_MAGIC_POWERBOARD: Mutex<Option<Arc<IioThread>>> = Mutex::new(None);

pub struct CalibratedChannelInner {
    name: &'static str,
    timebase: Instant,
    state: AtomicBool,
    last_poll_ms: AtomicU64,
    value: AtomicU32,
    nominal_value_on: f32,
    nominal_value_off: f32,
    noise: f32,
    time_constant_on: f32,
    time_constant_off: f32,
    parents: Vec<CalibratedChannel>,
}

#[derive(Clone)]
pub struct CalibratedChannel {
    inner: Arc<CalibratedChannelInner>,
}

impl CalibratedChannel {
    pub fn with_exponential(
        name: &'static str,
        nominal_value_on: f32,
        nominal_value_off: f32,
        noise: f32,
        time_constant_on: f32,
        time_constant_off: f32,
    ) -> Self {
        Self {
            inner: Arc::new(CalibratedChannelInner {
                name,
                timebase: Instant::now(),
                state: AtomicBool::new(false),
                last_poll_ms: AtomicU64::new(0),
                value: AtomicU32::new(nominal_value_off.to_bits()),
                nominal_value_on,
                nominal_value_off,
                noise,
                time_constant_on,
                time_constant_off,
                parents: Vec::new(),
            }),
        }
    }

    pub fn with_parents(name: &'static str, parents: Vec<CalibratedChannel>) -> Self {
        Self {
            inner: Arc::new(CalibratedChannelInner {
                name,
                timebase: Instant::now(),
                state: AtomicBool::new(false),
                last_poll_ms: AtomicU64::new(0),
                value: AtomicU32::new(0),
                nominal_value_on: 0.0,
                nominal_value_off: 0.0,
                noise: 0.0,
                time_constant_on: 0.0,
                time_constant_off: 0.0,
                parents,
            }),
        }
    }

    pub fn try_get_multiple<const N: usize>(
        &self,
        channels: [&Self; N],
    ) -> Result<[Measurement; N]> {
        let ts = Timestamp::now();
        let mut results = [Measurement { ts, value: 0.0 }; N];

        for i in 0..N {
            results[i].value = channels[i].get().unwrap().value;
        }

        Ok(results)
    }

    pub fn get(&self) -> Result<Measurement> {
        let ts = Timestamp::now();

        let dt = {
            let runtime = ts.as_instant().duration_since(self.inner.timebase);

            let runtime_ms = u64::try_from(runtime.as_millis()).unwrap();
            let last_poll_ms = self.inner.last_poll_ms.swap(runtime_ms, Ordering::Relaxed);

            (runtime_ms - last_poll_ms) as f32 / 1000.0
        };

        let (nominal, time_constant) = match self.inner.state.load(Ordering::Relaxed) {
            true => (self.inner.nominal_value_on, self.inner.time_constant_on),
            false => (self.inner.nominal_value_off, self.inner.time_constant_off),
        };

        let mut value = f32::from_bits(self.inner.value.load(Ordering::Relaxed));

        let decay = if time_constant.abs() < 0.01 {
            0.0
        } else {
            (-dt / time_constant).exp()
        };

        value -= nominal;
        value *= decay;
        value += (2.0 * random::<f32>() - 1.0) * self.inner.noise;
        value += self
            .inner
            .parents
            .iter()
            .map(|p| p.get().unwrap().value)
            .sum::<f32>();
        value += nominal;

        self.inner.value.store(value.to_bits(), Ordering::Relaxed);

        Ok(Measurement { ts, value })
    }

    pub fn set(&self, state: bool) {
        self.inner.state.store(state, Ordering::Relaxed);
    }
}

pub struct IioThread {
    channels: Vec<CalibratedChannel>,
}

impl IioThread {
    pub async fn new_stm32<W, G>(_wtb: &W, _hardware_generation: G) -> Result<Arc<Self>> {
        let mut demo_magic = block_on(DEMO_MAGIC_STM32.lock());

        // Only ever set up a single demo_mode "IioThread" per ADC
        if let Some(this) = &*demo_magic {
            return Ok(this.clone());
        }

        let usb_host_curr = CalibratedChannel::with_parents(
            "usb-host-curr",
            vec![
                CalibratedChannel::with_exponential("usb-host1-curr", 0.15, 0.005, 0.005, 0.3, 0.2),
                CalibratedChannel::with_exponential("usb-host2-curr", 0.2, 0.005, 0.005, 0.3, 0.2),
                CalibratedChannel::with_exponential("usb-host3-curr", 0.3, 0.005, 0.005, 0.3, 0.2),
            ],
        );

        let channels = vec![
            usb_host_curr.clone(),
            usb_host_curr.inner.parents[0].clone(),
            usb_host_curr.inner.parents[1].clone(),
            usb_host_curr.inner.parents[2].clone(),
            CalibratedChannel::with_exponential("out0-volt", 0.0, 3.3, 0.002, 0.1, 0.2),
            CalibratedChannel::with_exponential("out1-volt", 0.0, -3.3, 0.002, 0.2, 0.1),
            CalibratedChannel::with_exponential("iobus-curr", 0.15, 0.0, 0.001, 0.2, 0.01),
            CalibratedChannel::with_exponential("iobus-volt", 12.2, 0.0, 0.1, 0.2, 1.0),
        ];

        let this = Arc::new(Self { channels });

        *demo_magic = Some(this.clone());

        Ok(this)
    }

    pub async fn new_powerboard<W, G>(_wtb: &W, _hardware_generation: G) -> Result<Arc<Self>> {
        let mut demo_magic = block_on(DEMO_MAGIC_POWERBOARD.lock());

        // Only ever set up a single demo_mode "IioThread" per ADC
        if let Some(this) = &*demo_magic {
            return Ok(this.clone());
        }

        let channels = vec![
            CalibratedChannel::with_exponential("pwr-volt", 24.0, 0.0, 0.02, 0.2, 2.0),
            CalibratedChannel::with_exponential("pwr-curr", 1.2, 0.0, 0.002, 0.2, 0.01),
        ];

        let this = Arc::new(Self { channels });

        *demo_magic = Some(this.clone());

        Ok(this)
    }

    pub fn get_channel(self: Arc<Self>, ch_name: &str) -> Result<CalibratedChannel> {
        self.channels
            .iter()
            .find(|chan| chan.inner.name == ch_name)
            .ok_or(anyhow!("Could not get adc channel {}", ch_name))
            .cloned()
    }
}
