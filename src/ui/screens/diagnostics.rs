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
use crate::broker::Topic;

const SCREEN_TYPE: AlertScreen = AlertScreen::Diagnostics;

pub struct DiagnosticsScreen;

struct Active {
    display: Option<Display>,
    alerts: Arc<Topic<AlertList>>,
}

fn diagnostic_text() -> Result<String, std::fmt::Error> {
    let mut text = String::new();

    writeln!(&mut text, "Diagnostics | Not self-updating!")?;
    writeln!(&mut text, "Short press lower button to toggle LEDs.")?;
    writeln!(&mut text, "Long press lower button to exit.")?;
    writeln!(&mut text)?;

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

        let text = diagnostic_text().unwrap_or_else(|_| "Failed to format text".into());

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
