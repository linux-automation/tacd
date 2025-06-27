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

use std::fmt::Write;
use std::io::Result;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{Brightness, Leds, SysClass};

// These are traits that could in theory be contributed upstream to the sysfs_class
// crate, but the API is currently an ad-hoc creation.
// Contribute this upstream once we have an API that we think makes sense.

pub trait RgbColor: SysClass {
    fn set_rgb_color(&self, r: u64, g: u64, b: u64) -> Result<()>;
}

impl RgbColor for Leds {
    fn set_rgb_color(&self, r: u64, g: u64, b: u64) -> Result<()> {
        let multi_intensity: String = self
            .read_file("multi_index")?
            .split_whitespace()
            .map(|color_name| match color_name {
                "red" => format!("{r} "),
                "green" => format!("{g} "),
                "blue" => format!("{b} "),
                _ => panic!(),
            })
            .collect();

        self.write_file("multi_intensity", multi_intensity)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BlinkPattern {
    repetitions: i32,
    steps: Vec<(f32, Duration)>,
}

impl BlinkPattern {
    pub fn solid(val: f32) -> Self {
        Self {
            repetitions: 1,
            steps: vec![
                (val, Duration::from_millis(1000)),
                (val, Duration::from_millis(1000)),
            ],
        }
    }

    #[cfg(test)]
    pub fn is_on(&self) -> bool {
        self.steps.iter().all(|(brightness, _)| *brightness >= 0.5)
    }

    #[cfg(test)]
    pub fn is_off(&self) -> bool {
        self.steps.iter().all(|(brightness, _)| *brightness < 0.5)
    }

    #[cfg(test)]
    pub fn is_blinking(&self) -> bool {
        !(self.is_on() || self.is_off())
    }
}

pub struct BlinkPatternBuilder {
    value: f32,
    pattern: BlinkPattern,
}

impl BlinkPatternBuilder {
    pub fn new(initial: f32) -> Self {
        Self {
            value: initial,
            pattern: BlinkPattern {
                repetitions: 0,
                steps: Vec::new(),
            },
        }
    }

    pub fn fade_to(mut self, brightness: f32, duration: Duration) -> Self {
        self.value = brightness;
        self.pattern.steps.push((brightness, duration));
        self
    }

    pub fn step_to(self, brightness: f32) -> Self {
        self.fade_to(brightness, Duration::ZERO)
    }

    pub fn stay_for(self, duration: Duration) -> Self {
        let value = self.value;
        self.fade_to(value, duration)
    }

    pub fn repeat(mut self, repetitions: i32) -> BlinkPattern {
        self.pattern.repetitions = repetitions;
        self.pattern
    }

    #[allow(dead_code)]
    pub fn once(self) -> BlinkPattern {
        self.repeat(1)
    }

    pub fn forever(self) -> BlinkPattern {
        self.repeat(-1)
    }
}

pub trait Pattern: SysClass {
    fn set_pattern(&self, pattern: BlinkPattern) -> Result<()>;
}

impl Pattern for Leds {
    fn set_pattern(&self, pattern: BlinkPattern) -> Result<()> {
        let max = self.max_brightness()? as f32;
        let repetitions = pattern.repetitions;
        let pattern =
            pattern
                .steps
                .iter()
                .fold(String::new(), |mut dst, (brightness, duration)| {
                    let brightness = (brightness * max).round();
                    let duration = duration.as_millis();

                    write!(dst, "{brightness} {duration} ")
                        .expect("Writing to a String should never fail");

                    dst
                });

        self.write_file("trigger", "pattern")?;
        self.write_file("pattern", pattern)?;
        self.write_file("repeat", repetitions.to_string())
    }
}
