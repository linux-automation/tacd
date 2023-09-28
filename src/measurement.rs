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
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use std::ops::{Deref, DerefMut};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy)]
pub struct Timestamp(Instant);

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Measurement {
    pub ts: Timestamp,
    pub value: f32,
}

impl Measurement {
    pub fn now(value: f32) -> Self {
        Self {
            ts: Timestamp::now(),
            value,
        }
    }
}

impl Timestamp {
    pub fn new(inst: Instant) -> Self {
        Self(inst)
    }

    pub fn now() -> Self {
        Self::new(Instant::now())
    }

    pub fn as_instant(self) -> Instant {
        self.0
    }

    /// Represent a Timestamp in system time
    /// Since Instants use a monotonic clock that is not actually related to the
    /// system clock this is a somewhat handwavey process.
    ///
    /// The idea is to take the current Instant (monotonic time) and System Time
    /// (calendar time) and calculate: now_system - (now_instant - ts_instant).
    pub fn in_system_time(&self) -> Result<SystemTime> {
        let age = self.0.elapsed();
        SystemTime::now()
            .checked_sub(age)
            .with_context(|| "couldn't get system time")
    }
}

impl Deref for Timestamp {
    type Target = Instant;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Timestamp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Serialize for Timestamp {
    /// Serialize an Instant as a javascript timestamp (f64 containing the number
    /// of milliseconds since Unix Epoch 0).
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::Error;
        match {
            || {
                let time = self.in_system_time()?;
                let timestamp = time.duration_since(SystemTime::UNIX_EPOCH)?;
                let js_timestamp = 1000.0 * timestamp.as_secs_f64();
                anyhow::Ok(js_timestamp)
            }
        }() {
            Ok(timestamp) => timestamp.serialize(serializer),
            Err(e) => Err(Error::custom(format!(
                "failed to serialize Timestamp with {e}"
            ))),
        }
    }
}

impl<'d> Deserialize<'d> for Timestamp {
    fn deserialize<D>(_: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'d>,
    {
        use serde::de::Error;
        Err(Error::custom("unused implementation"))
    }
}
