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

use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Text},
};

use super::widgets::*;
use super::{ActivatableScreen, ActiveScreen, Display, InputEvent, Screen, Ui};
use crate::broker::Topic;

const SCREEN_TYPE: Screen = Screen::RebootConfirm;

pub struct RebootConfirmScreen;

impl RebootConfirmScreen {
    pub fn new() -> Self {
        Self
    }
}

fn rly(display: &Display) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    display.with_lock(|target| {
        Text::with_alignment(
            "Really reboot?\nLong press lower\nbutton to confirm.",
            Point::new(120, 120),
            text_style,
            Alignment::Center,
        )
        .draw(target)
        .unwrap();
    });
}

fn brb(display: &Display) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    display.clear();

    display.with_lock(|target| {
        Text::with_alignment(
            "Hold tight\nBe right back",
            Point::new(120, 120),
            text_style,
            Alignment::Center,
        )
        .draw(target)
        .unwrap();
    });
}

struct Active {
    display: Display,
    reboot: Arc<Topic<bool>>,
    screen: Arc<Topic<Screen>>,
}

impl ActivatableScreen for RebootConfirmScreen {
    fn my_type(&self) -> Screen {
        SCREEN_TYPE
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        rly(&display);

        let reboot = ui.res.systemd.reboot.clone();
        let screen = ui.screen.clone();

        let active = Active {
            display,
            reboot,
            screen,
        };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    async fn deactivate(mut self: Box<Self>) -> Display {
        self.display
    }

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen | InputEvent::ToggleAction(_) => {
                self.screen.set(SCREEN_TYPE.next())
            }
            InputEvent::PerformAction(_) => {
                brb(&self.display);
                self.reboot.set(true);
            }
        }
    }
}
