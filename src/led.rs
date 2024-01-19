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

use std::io::ErrorKind;

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use log::{error, info, warn};

use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

mod demo_mode;
mod extras;

#[cfg(feature = "demo_mode")]
use demo_mode::{Brightness, Leds, SysClass};

#[cfg(not(feature = "demo_mode"))]
use sysfs_class::{Brightness, Leds, SysClass};

pub use extras::{BlinkPattern, BlinkPatternBuilder};
use extras::{Pattern, RgbColor};

pub struct Led {
    pub out_0: Arc<Topic<BlinkPattern>>,
    pub out_1: Arc<Topic<BlinkPattern>>,
    pub dut_pwr: Arc<Topic<BlinkPattern>>,
    pub eth_dut: Arc<Topic<BlinkPattern>>,
    pub eth_lab: Arc<Topic<BlinkPattern>>,
    pub status: Arc<Topic<BlinkPattern>>,
    pub status_color: Arc<Topic<(f32, f32, f32)>>,
}

/// Get the specified LED and output an appropriate message if it fails
///
/// Different versions of the hardware have different amounts of on-board LEDs,
/// so not finding an LED should not be a critical error.
/// Just show a not and go on if an LED can not be set up.
fn get_led_checked(hardware_name: &'static str) -> Option<Leds> {
    match Leds::new(hardware_name) {
        Ok(led) => Some(led),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            info!("Hardware does not have LED {hardware_name}, ignoring");
            None
        }
        Err(err) => {
            error!("Failed to set up LED {hardware_name}: {err}");
            None
        }
    }
}

fn handle_pattern(
    bb: &mut BrokerBuilder,
    wtb: &mut WatchedTasksBuilder,
    hardware_name: &'static str,
    topic_name: &'static str,
) -> Result<Arc<Topic<BlinkPattern>>> {
    let topic = bb.topic_ro(&format!("/v1/tac/led/{topic_name}/pattern"), None);

    if let Some(led) = get_led_checked(hardware_name) {
        let (mut rx, _) = topic.clone().subscribe_unbounded();

        wtb.spawn_task("led-pattern-update", async move {
            while let Some(pattern) = rx.next().await {
                if let Err(e) = led.set_pattern(pattern) {
                    warn!("Failed to set LED pattern: {}", e);
                }
            }

            Ok(())
        })?;
    }

    Ok(topic)
}

fn handle_color(
    bb: &mut BrokerBuilder,
    wtb: &mut WatchedTasksBuilder,
    hardware_name: &'static str,
    topic_name: &'static str,
) -> Result<Arc<Topic<(f32, f32, f32)>>> {
    let topic = bb.topic_ro(&format!("/v1/tac/led/{topic_name}/color"), None);

    if let Some(led) = get_led_checked(hardware_name) {
        let (mut rx, _) = topic.clone().subscribe_unbounded();

        wtb.spawn_task("led-color-update", async move {
            while let Some((r, g, b)) = rx.next().await {
                let max = led.max_brightness()?;

                // I've encountered LEDs staying off when set to the max value,
                // but setting them to (max - 1) turned them on.
                let max = (max - 1) as f32;

                if let Err(e) = led.set_rgb_color((r * max) as _, (g * max) as _, (b * max) as _) {
                    warn!("Failed to set LED color: {}", e);
                }
            }

            Ok(())
        })?;
    }

    Ok(topic)
}

impl Led {
    pub fn new(bb: &mut BrokerBuilder, wtb: &mut WatchedTasksBuilder) -> Result<Self> {
        Ok(Self {
            out_0: handle_pattern(bb, wtb, "tac:green:out0", "out_0")?,
            out_1: handle_pattern(bb, wtb, "tac:green:out1", "out_1")?,
            dut_pwr: handle_pattern(bb, wtb, "tac:green:dutpwr", "dut_pwr")?,
            eth_dut: handle_pattern(bb, wtb, "tac:green:statusdut", "eth_dut")?,
            eth_lab: handle_pattern(bb, wtb, "tac:green:statuslab", "eth_lab")?,
            status: handle_pattern(bb, wtb, "rgb:status", "status")?,
            status_color: handle_color(bb, wtb, "rgb:status", "status")?,
        })
    }
}
