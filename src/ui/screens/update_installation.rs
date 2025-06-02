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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::prelude::*;

use super::widgets::*;
use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display, InputEvent, Screen,
    Ui,
};
use crate::broker::Topic;
use crate::dbus::rauc::Progress;
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::UpdateInstallation;
const REBOOT_MESSAGE: &str = "There is a newer
OS install in
another slot.

Long Press to
boot it.
";

pub struct UpdateInstallationScreen;

struct Active {
    widgets: WidgetContainer,
}

impl UpdateInstallationScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        operation: &Arc<Topic<String>>,
        reboot_message: &Arc<Topic<Option<String>>>,
        should_reboot: &Arc<Topic<bool>>,
    ) -> Result<Self> {
        let (mut operation_events, _) = operation.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-update-activator", async move {
            while let Some(ev) = operation_events.next().await {
                match ev.as_str() {
                    "installing" => alerts.assert(SCREEN_TYPE),
                    _ => alerts.deassert(SCREEN_TYPE),
                };
            }

            Ok(())
        })?;

        let (mut should_reboot_events, _) = should_reboot.clone().subscribe_unbounded();
        let reboot_message = reboot_message.clone();

        wtb.spawn_task("screen-update-should-reboot", async move {
            while let Some(should_reboot) = should_reboot_events.next().await {
                if should_reboot {
                    reboot_message.set(Some(REBOOT_MESSAGE.to_string()))
                }
            }

            Ok(())
        })?;

        Ok(Self)
    }
}

impl ActivatableScreen for UpdateInstallationScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        // This screen is left automatically once the update is complete.
        // No way to exit it prior to that.
        display.with_lock(|target| draw_button_legend(target, "-", "-"));

        let mut widgets = WidgetContainer::new(display);

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

        Box::new(Active { widgets })
    }
}

#[async_trait]
impl ActiveScreen for Active {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    async fn deactivate(mut self: Box<Self>) -> Display {
        self.widgets.destroy().await
    }

    fn input(&mut self, _ev: InputEvent) {}
}
