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

use std::time::Instant;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::{Alignment, Text},
};

use super::widgets::*;
use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display, InputEvent, Screen,
    Ui,
};
use crate::broker::Topic;
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::Locator;

pub struct LocatorScreen;

struct Active {
    locator: Arc<Topic<bool>>,
    widgets: WidgetContainer,
}

impl LocatorScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        locator: &Arc<Topic<bool>>,
    ) -> Self {
        let (mut locator_events, _) = locator.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-locator-activator", async move {
            while let Some(locator) = locator_events.next().await {
                if locator {
                    alerts.assert(SCREEN_TYPE);
                } else {
                    alerts.deassert(SCREEN_TYPE);
                }
            }

            Ok(())
        });

        Self
    }
}

impl ActivatableScreen for LocatorScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            Text::with_alignment(
                "Locating this TAC",
                Point::new(120, 80),
                ui_text_style,
                Alignment::Center,
            )
            .draw(target)
            .unwrap();

            Text::with_alignment(
                "> Found it!",
                Point::new(120, 200),
                ui_text_style,
                Alignment::Center,
            )
            .draw(target)
            .unwrap();
        });

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| {
            DynamicWidget::text_center(
                ui.res.hostname.hostname.clone(),
                display,
                Point::new(120, 130),
                Box::new(|hostname| hostname.clone()),
            )
        });

        let start = Instant::now();

        widgets.push(|display| {
            DynamicWidget::new(
                ui.res.adc.time.clone(),
                display,
                Box::new(move |now, target| {
                    // Blink a bar below the hostname at 2Hz
                    let on = (now.duration_since(start).as_millis() / 500) % 2 == 0;

                    if on {
                        let line = Line::new(Point::new(40, 135), Point::new(200, 135))
                            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2));

                        line.draw(target).unwrap();

                        Some(line.bounding_box())
                    } else {
                        None
                    }
                }),
            )
        });

        let locator = ui.locator.clone();

        let active = Active { locator, widgets };

        Box::new(active)
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

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen => {}
            InputEvent::ToggleAction(_) => {}
            InputEvent::PerformAction(_) => {
                self.locator.set(false);
            }
        }
    }
}
