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

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use log::warn;

mod demo_mode;

#[cfg(feature = "demo_mode")]
use demo_mode::{Backlight as SysBacklight, Brightness, SysClass};

#[cfg(not(feature = "demo_mode"))]
use sysfs_class::{Backlight as SysBacklight, Brightness, SysClass};

use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

pub struct Backlight {
    pub brightness: Arc<Topic<f32>>,
}

impl Backlight {
    pub fn new(bb: &mut BrokerBuilder, wtb: &mut WatchedTasksBuilder) -> Result<Self> {
        let brightness = bb.topic_rw("/v1/tac/display/backlight/brightness", Some(1.0));

        let (mut rx, _) = brightness.clone().subscribe_unbounded();

        let backlight = SysBacklight::new("backlight")?;
        let max_brightness = backlight.max_brightness()?;

        wtb.spawn_task("backlight-dimmer", async move {
            while let Some(fraction) = rx.next().await {
                let brightness = (max_brightness as f32) * fraction;
                let mut brightness = brightness.clamp(0.0, max_brightness as f32) as u64;

                // A brightness of 0 turns the backlight off completely.
                // If the user selects something low but not zero they likely
                // want a dim glow, not completely off.
                if fraction > 0.01 && brightness == 0 {
                    brightness = 1;
                }

                if let Err(e) = backlight.set_brightness(brightness) {
                    warn!("Failed to set LED pattern: {}", e);
                }
            }

            Ok(())
        });

        Ok(Self { brightness })
    }
}
