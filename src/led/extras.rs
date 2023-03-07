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
    #[allow(dead_code)]
    pub fn solid(val: f32) -> Self {
        Self {
            repetitions: 1,
            steps: vec![
                (val, Duration::from_millis(1000)),
                (val, Duration::from_millis(1000)),
            ],
        }
    }
}

pub struct BlinkPatternBuilder {
    #[allow(dead_code)]
    value: f32,
    pattern: BlinkPattern,
}

impl BlinkPatternBuilder {
    #[allow(dead_code)]
    pub fn new(initial: f32) -> Self {
        Self {
            value: initial,
            pattern: BlinkPattern {
                repetitions: 0,
                steps: Vec::new(),
            },
        }
    }

    #[allow(dead_code)]
    pub fn fade_to(mut self, brightness: f32, duration: Duration) -> Self {
        self.value = brightness;
        self.pattern.steps.push((brightness, duration));
        self
    }

    #[allow(dead_code)]
    pub fn step_to(self, brightness: f32) -> Self {
        self.fade_to(brightness, Duration::ZERO)
    }

    #[allow(dead_code)]
    pub fn stay_for(self, duration: Duration) -> Self {
        let value = self.value;
        self.fade_to(value, duration)
    }

    #[allow(dead_code)]
    pub fn repeat(mut self, repetitions: i32) -> BlinkPattern {
        self.pattern.repetitions = repetitions;
        self.pattern
    }

    #[allow(dead_code)]
    pub fn once(self) -> BlinkPattern {
        self.repeat(1)
    }

    #[allow(dead_code)]
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
        let pattern: String = pattern
            .steps
            .iter()
            .map(|(brightness, duration)| {
                let brightness = (brightness * max).round();
                let duration = duration.as_millis();
                format!("{} {} ", brightness, duration)
            })
            .collect();

        self.write_file("trigger", "pattern")?;
        self.write_file("pattern", pattern)?;
        self.write_file("repeat", repetitions.to_string())
    }
}
