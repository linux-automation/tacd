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

use std::sync::Arc;

use async_std::prelude::*;
use async_std::task::spawn;
use async_trait::async_trait;
use embedded_graphics::prelude::Point;

use super::buttons::*;
use super::widgets::*;
use super::{Display, MountableScreen, Screen, Ui};
use crate::broker::{Native, SubscriptionHandle, Topic};

const SCREEN_TYPE: Screen = Screen::Help;
const PAGES: &[&str] = &[
    "Hey there!

A short guide on how
this interface works:
Long presses on the
lower button perform
actions.
...",
    "...

Short presses on the
lower button toggle
between actions.

...",
    "...

And the upper button
switches to the next
screen.

Press it to leave
this guide",
];

pub struct HelpScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl HelpScreen {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for HelpScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui, display: Arc<Display>) {
        let up = Topic::anonymous(Some(false));
        let page = Topic::anonymous(Some(0));

        self.widgets.push(Box::new(DynamicWidget::text(
            page.clone(),
            display.clone(),
            Point::new(8, 24),
            Box::new(|page| PAGES[*page].into()),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            up.clone(),
            display.clone(),
            Point::new(8, 200),
            Box::new(|up| match up {
                false => "  Scroll up".into(),
                true => "> Scroll up".into(),
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            up.clone(),
            display,
            Point::new(8, 220),
            Box::new(|up| match up {
                false => "> Scroll down".into(),
                true => "  Scroll down".into(),
            }),
        )));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                        src: _,
                    } => up.toggle(false),
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                        src: _,
                    } => {
                        let up = up.clone().get().await;

                        page.modify(|page| match (page.unwrap_or(0), up) {
                            (0, true) => Some(0),
                            (p, true) => Some(p - 1),
                            (2, false) => Some(2),
                            (p, false) => Some(p + 1),
                        });
                    }
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                        src: _,
                    } => {
                        screen.set(SCREEN_TYPE.next());
                    }
                    ButtonEvent::Press { btn: _, src: _ } => {}
                }
            }
        });

        self.buttons_handle = Some(buttons_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.buttons_handle.take() {
            handle.unsubscribe();
        }

        for mut widget in self.widgets.drain(..) {
            widget.unmount().await
        }
    }
}
