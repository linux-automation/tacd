// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2024 Pengutronix e.K.
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

use async_std::sync::Arc;
use async_trait::async_trait;
use chrono::DateTime;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};

use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display, InputEvent, Screen,
    Ui,
};
use crate::{broker::Topic, led::BlinkPattern, system::HardwareGeneration};

const SCREEN_TYPE: AlertScreen = AlertScreen::Diagnostics;

pub struct DiagnosticsScreen;

struct Active {
    display: Option<Display>,
    alerts: Arc<Topic<AlertList>>,
    led_cycle_state: u8,
    leds: [Arc<Topic<BlinkPattern>>; 5],
    status_led_color: Arc<Topic<(f32, f32, f32)>>,
    backlight_brightness: Arc<Topic<f32>>,
}

fn diagnostic_text(ui: &Ui) -> Result<String, std::fmt::Error> {
    let mut text = String::new();

    writeln!(&mut text, "Diagnostics | Not self-updating!")?;
    writeln!(&mut text, "Short press lower button to toggle LEDs.")?;
    writeln!(&mut text, "Long press lower button to exit.")?;
    writeln!(&mut text)?;

    if let Some(tacd_version) = ui.res.system.tacd_version.try_get() {
        writeln!(&mut text, "v: {tacd_version}")?;
    }

    if let Some(uname) = ui.res.system.uname.try_get() {
        writeln!(&mut text, "uname: {} {} ", uname.nodename, uname.release)?;
    }

    if let Some(hardware_generation) = ui.res.system.hardware_generation.try_get() {
        let gen = match hardware_generation {
            HardwareGeneration::Gen1 => "Gen1",
            HardwareGeneration::Gen2 => "Gen2",
            HardwareGeneration::Gen3 => "Gen3",
        };

        write!(&mut text, "generation: {gen} ")?;
    }

    if let Some(soc_temperature) = ui.res.temperatures.soc_temperature.try_get() {
        write!(&mut text, "temperature: {} C", soc_temperature.value)?;
    }

    writeln!(&mut text)?;

    if let Some(bridge_interface) = ui.res.network.bridge_interface.try_get() {
        write!(&mut text, "br: ")?;

        for ip in bridge_interface {
            write!(&mut text, "{ip}, ")?;
        }

        writeln!(&mut text)?;
    }

    let interfaces = [
        ("dut", &ui.res.network.dut_interface),
        ("uplink", &ui.res.network.uplink_interface),
    ];

    for (name, interface) in interfaces {
        if let Some(link) = interface.try_get() {
            let speed = link.speed;
            let carrier = if link.carrier { "up" } else { "down" };

            write!(&mut text, "{name}: {speed} {carrier} | ")?;
        }
    }

    writeln!(&mut text)?;
    writeln!(&mut text)?;

    if let Some(barebox) = ui.res.system.barebox.try_get() {
        let baseboard_release = barebox.baseboard_release.trim_start_matches("lxatac-");
        let powerboard_release = barebox.powerboard_release.trim_start_matches("lxatac-");
        let baseboard_timestamp = DateTime::from_timestamp(barebox.baseboard_timestamp as i64, 0)
            .map_or_else(|| "???".to_string(), |ts| ts.to_rfc3339());
        let powerboard_timestamp = DateTime::from_timestamp(barebox.powerboard_timestamp as i64, 0)
            .map_or_else(|| "???".to_string(), |ts| ts.to_rfc3339());
        let baseboard_featureset = barebox.baseboard_featureset.join(",");
        let powerboard_featureset = barebox.powerboard_featureset.join(",");

        writeln!(&mut text, "barebox: {}", barebox.version)?;
        writeln!(&mut text, "baseboard ({baseboard_release}):")?;
        writeln!(&mut text, "- bringup: {baseboard_timestamp}")?;
        writeln!(&mut text, "- feat: {baseboard_featureset}")?;
        writeln!(&mut text, "powerboard ({powerboard_release}):")?;
        writeln!(&mut text, "- bringup: {powerboard_timestamp}")?;
        writeln!(&mut text, "- feat: {powerboard_featureset}")?;
    }

    writeln!(&mut text)?;

    if let Some(channels) = ui.res.rauc.channels.try_get() {
        write!(&mut text, "chs: ")?;

        for ch in channels {
            let en = if ch.enabled { "[x]" } else { "[ ]" };
            let name = ch.name;

            write!(&mut text, "{en} {name}, ")?;
        }

        writeln!(&mut text)?;
    }

    if let Some(slot_status) = ui.res.rauc.slot_status.try_get() {
        let primary = ui.res.rauc.primary.try_get();

        for fs in ["rootfs_0", "rootfs_1"] {
            let rootfs = slot_status.get(fs);

            let bundle_version = rootfs
                .and_then(|r| r.get("bundle_version"))
                .map(|s| s.as_str())
                .unwrap_or("?");
            let state = rootfs
                .and_then(|r| r.get("state"))
                .map(|s| s.as_str())
                .unwrap_or("?");
            let boot_status = rootfs
                .and_then(|r| r.get("boot_status"))
                .map(|s| s.as_str())
                .unwrap_or("?");
            let status = rootfs
                .and_then(|r| r.get("status"))
                .map(|s| s.as_str())
                .unwrap_or("?");

            let is_primary = primary.as_ref().is_some_and(|p| p == fs);

            // Do do not have much space. Use compact representations.
            let primary_marker = if is_primary { "[x]" } else { "[ ]" };
            let fs = fs.trim_start_matches("root");
            let state = &state[..2];

            writeln!(
                &mut text,
                "{primary_marker} {fs} {state} {boot_status} {status} {bundle_version}"
            )?;
        }
    }

    Ok(text)
}

impl DiagnosticsScreen {
    pub fn new() -> Self {
        Self
    }
}

impl ActivatableScreen for DiagnosticsScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

        let text = diagnostic_text(ui).unwrap_or_else(|_| "Failed to format text".into());

        display.with_lock(|target| {
            Rectangle::with_corners(Point::new(0, 0), Point::new(239, 239))
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                .draw(target)
                .unwrap();

            Text::with_baseline(&text, Point::new(4, 2), ui_text_style, Baseline::Top)
                .draw(target)
                .unwrap();
        });

        let leds = [
            ui.res.led.out_0.clone(),
            ui.res.led.out_1.clone(),
            ui.res.led.dut_pwr.clone(),
            ui.res.led.eth_dut.clone(),
            ui.res.led.eth_lab.clone(),
        ];

        // Set the status LED to maximum brightness.
        // (The actual appearance is controlled via the RGB color value)
        ui.res.led.status.set(BlinkPattern::solid(1.0));

        let status_led_color = ui.res.led.status_color.clone();

        let backlight_brightness = ui.res.backlight.brightness.clone();

        let active = Active {
            display: Some(display),
            alerts: ui.alerts.clone(),
            led_cycle_state: 0,
            leds,
            status_led_color,
            backlight_brightness,
        };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    async fn deactivate(mut self: Box<Self>) -> Display {
        self.backlight_brightness.set(1.0);
        self.display.take().unwrap()
    }

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen => {}
            InputEvent::ToggleAction(_) => {
                self.led_cycle_state = self.led_cycle_state.wrapping_add(1);

                let on = !self.led_cycle_state.is_multiple_of(2);
                let led_brightness = if on { 1.0 } else { 0.0 };
                let backlight_brightness = if on { 1.0 } else { 0.1 };
                let status_color = match self.led_cycle_state % 8 {
                    0 => (0.0, 1.0, 0.0),
                    1 => (0.0, 0.0, 1.0),
                    2 => (1.0, 0.0, 0.0),
                    3 => (1.0, 1.0, 0.0),
                    4 => (1.0, 0.0, 1.0),
                    5 => (0.0, 1.0, 1.0),
                    6 => (1.0, 1.0, 1.0),
                    7 => (0.0, 0.0, 0.0),
                    _ => unreachable!(),
                };

                self.status_led_color.set(status_color);

                for led in &self.leds {
                    led.set(BlinkPattern::solid(led_brightness));
                }

                self.backlight_brightness.set(backlight_brightness);
            }
            InputEvent::PerformAction(_) => {
                self.alerts.deassert(SCREEN_TYPE);
            }
        }
    }
}
