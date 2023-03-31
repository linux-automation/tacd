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

use async_std::prelude::*;
use async_std::task::spawn;
use async_trait::async_trait;

use crate::broker::{Native, SubscriptionHandle};
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Text},
};

use super::buttons::*;
use super::widgets::*;
use super::{FramebufferDrawTarget, MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::RebootConfirm;

pub struct RebootConfirmScreen {
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl RebootConfirmScreen {
    pub fn new() -> Self {
        Self {
            buttons_handle: None,
        }
    }
}

fn rly(draw_target: &mut FramebufferDrawTarget) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    Text::with_alignment(
        "Really reboot?\nLong press to confirm",
        Point::new(120, 120),
        text_style,
        Alignment::Center,
    )
    .draw(draw_target)
    .unwrap();
}

fn brb(draw_target: &mut FramebufferDrawTarget) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    draw_target.clear();

    Text::with_alignment(
        "Hold tight\nBe right back",
        Point::new(120, 120),
        text_style,
        Alignment::Center,
    )
    .draw(draw_target)
    .unwrap();
}

#[async_trait]
impl MountableScreen for RebootConfirmScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        let draw_target = ui.draw_target.clone();
        rly(&mut *draw_target.lock().await);

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let screen = ui.screen.clone();
        let reboot = ui.res.systemd.reboot.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                    } => {
                        brb(&mut *draw_target.lock().await);
                        reboot.set(true);
                        break;
                    }
                    ButtonEvent::Press { btn: _ } => {}
                    _ => screen.set(SCREEN_TYPE.next()),
                }
            }
        });

        self.buttons_handle = Some(buttons_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.buttons_handle.take() {
            handle.unsubscribe();
        }
    }
}
