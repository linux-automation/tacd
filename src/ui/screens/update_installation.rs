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
use crate::dbus::rauc::Progress;

use super::widgets::*;
use super::{ActivatableScreen, ActiveScreen, Display, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::UpdateInstallation;

pub struct UpdateInstallationScreen;

impl UpdateInstallationScreen {
    pub fn new(screen: &Arc<Topic<Screen>>, operation: &Arc<Topic<String>>) -> Self {
        // Activate the rauc screen if an update is started and deactivate
        // if it is done
        let screen = screen.clone();
        let (mut operation_events, _) = operation.clone().subscribe_unbounded();

        spawn(async move {
            let mut operation_prev = operation_events.next().await.unwrap();

            while let Some(ev) = operation_events.next().await {
                match (operation_prev.as_str(), ev.as_str()) {
                    (_, "installing") => screen.set(SCREEN_TYPE),
                    ("installing", _) => screen.set(SCREEN_TYPE.next()),
                    _ => {}
                };

                operation_prev = ev;
            }
        });

        Self
    }
}

struct Active {
    widgets: WidgetContainer,
}

impl ActivatableScreen for UpdateInstallationScreen {
    fn my_type(&self) -> Screen {
        SCREEN_TYPE
    }

    fn activate(&mut self, ui: &Ui, display: Arc<Display>) -> Box<dyn ActiveScreen> {
        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| DynamicWidget::locator(ui.locator_dance.clone(), display));

        widgets.push(|display| {
            DynamicWidget::text_center(
                ui.res.rauc.progress.clone(),
                display,
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
        });

        widgets.push(|display| {
            DynamicWidget::bar(
                ui.res.rauc.progress.clone(),
                display,
                Point::new(20, 180),
                200,
                18,
                Box::new(|progress: &Progress| progress.percentage as f32 / 100.0),
            )
        });

        let active = Active { widgets };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    async fn deactivate(mut self: Box<Self>) {
        self.widgets.destroy().await;
    }
}
