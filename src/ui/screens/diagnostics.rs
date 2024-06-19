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
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

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
use crate::{broker::Topic, system::HardwareGeneration};

const SCREEN_TYPE: AlertScreen = AlertScreen::Diagnostics;

pub struct DiagnosticsScreen;

struct Active {
    display: Option<Display>,
    alerts: Arc<Topic<AlertList>>,
}

fn diagnostic_text(ui: &Ui) -> Result<String, std::fmt::Error> {
    let mut text = String::new();

    writeln!(&mut text, "Diagnostics | Not self-updating!")?;
    writeln!(&mut text, "Short press lower button to toggle LEDs.")?;
    writeln!(&mut text, "Long press lower button to exit.")?;
    writeln!(&mut text)?;

    if let Some(tacd_version) = ui.res.system.tacd_version.try_get() {
        writeln!(&mut text, "v: {}", tacd_version)?;
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

        write!(&mut text, "generation: {} ", gen)?;
    }

    if let Some(soc_temperature) = ui.res.temperatures.soc_temperature.try_get() {
        write!(&mut text, "temperature: {} C", soc_temperature.value)?;
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
        writeln!(&mut text, "baseboard ({}):", baseboard_release)?;
        writeln!(&mut text, "- bringup: {}", baseboard_timestamp)?;
        writeln!(&mut text, "- feat: {}", baseboard_featureset)?;
        writeln!(&mut text, "powerboard ({}):", powerboard_release)?;
        writeln!(&mut text, "- bringup: {}", powerboard_timestamp)?;
        writeln!(&mut text, "- feat: {}", powerboard_featureset)?;
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

        let active = Active {
            display: Some(display),
            alerts: ui.alerts.clone(),
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
        self.display.take().unwrap()
    }

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen => {}
            InputEvent::ToggleAction(_) => {}
            InputEvent::PerformAction(_) => {
                self.alerts.deassert(SCREEN_TYPE);
            }
        }
    }
}
