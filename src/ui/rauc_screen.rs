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
use async_std::sync::Arc;
use async_std::task::spawn;
use async_trait::async_trait;

use embedded_graphics::prelude::*;

use crate::broker::Topic;
use crate::dbus::Progress;

use super::widgets::*;
use super::{MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::Rauc;

pub struct RaucScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
}

impl RaucScreen {
    pub fn new(screen: &Arc<Topic<Screen>>, operation: &Arc<Topic<String>>) -> Self {
        // Activate the rauc screen if an update is started and deactivate
        // if it is done
        let screen = screen.clone();
        let operation = operation.clone();

        spawn(async move {
            let mut operation_prev: Arc<String> = operation.get().await;
            let (mut operation_events, _) = operation.subscribe_unbounded().await;

            while let Some(ev) = operation_events.next().await {
                match (operation_prev.as_str(), ev.as_str()) {
                    (_, "installing") => screen.set(SCREEN_TYPE).await,
                    ("installing", _) => screen.set(SCREEN_TYPE.next()).await,
                    _ => {}
                };

                operation_prev = ev;
            }
        });

        Self {
            widgets: Vec::new(),
        }
    }
}

#[async_trait]
impl MountableScreen for RaucScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text_center(
                ui.res.dbus.rauc.progress.clone(),
                ui.draw_target.clone(),
                Point::new(120, 100),
                Box::new(|progress: &Progress| {
                    let (_, text) = progress.message.split_whitespace().fold(
                        (0, String::new()),
                        move |(mut ll, mut text), word| {
                            let word_len = word.len();

                            if (ll + word_len) > 15 {
                                text.push('\n');
                                ll = 0;
                            } else {
                                text.push(' ');
                                ll += 1;
                            }

                            text.push_str(word);
                            ll += word_len;

                            (ll, text)
                        },
                    );

                    text
                }),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::bar(
                ui.res.dbus.rauc.progress.clone(),
                ui.draw_target.clone(),
                Point::new(20, 180),
                200,
                18,
                Box::new(|progress: &Progress| progress.percentage as f32 / 100.0),
            )
            .await,
        ));
    }

    async fn unmount(&mut self) {
        for mut widget in self.widgets.drain(..) {
            widget.unmount().await
        }
    }
}
