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

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Text},
};

use super::widgets::*;
use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display, InputEvent, Screen,
    Ui,
};
use crate::broker::Topic;
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::RebootConfirm;

pub struct RebootConfirmScreen {
    reboot_message: Arc<Topic<Option<String>>>,
}

impl RebootConfirmScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        reboot_message: &Arc<Topic<Option<String>>>,
    ) -> Result<Self> {
        // Receive questions like Some("Do you want to reboot?") and activate this screen
        let (mut reboot_message_events, _) = reboot_message.clone().subscribe_unbounded();
        let reboot_message = reboot_message.clone();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-reboot-activator", async move {
            while let Some(reboot_message) = reboot_message_events.next().await {
                if reboot_message.is_some() {
                    alerts.assert(SCREEN_TYPE);
                } else {
                    alerts.deassert(SCREEN_TYPE);
                }
            }

            Ok(())
        })?;

        Ok(Self { reboot_message })
    }
}

fn rly(text: &str, display: &Display) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    display.with_lock(|target| {
        draw_button_legend(target, "Reboot", "Dismiss");

        Text::with_alignment(text, Point::new(115, 80), text_style, Alignment::Center)
            .draw(target)
            .unwrap()
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
    reboot_message: Arc<Topic<Option<String>>>,
}

impl ActivatableScreen for RebootConfirmScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let text = self.reboot_message.try_get().unwrap().unwrap();

        rly(&text, &display);

        let reboot = ui.res.systemd.reboot.clone();
        let reboot_message = self.reboot_message.clone();

        let active = Active {
            display,
            reboot,
            reboot_message,
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
        self.display
    }

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen => self.reboot_message.set(None),
            InputEvent::ToggleAction(_) => {}
            InputEvent::PerformAction(_) => {
                brb(&self.display);
                self.reboot.set(true);
            }
        }
    }
}
