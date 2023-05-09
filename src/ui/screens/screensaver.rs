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

use std::convert::TryInto;
use std::time::{Duration, SystemTime};

use async_std::future::timeout;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoFont, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};

use super::buttons::*;
use super::widgets::*;
use super::{ActivatableScreen, ActiveScreen, Display, InputEvent, Screen, Ui};
use crate::broker::Topic;

const UI_TEXT_FONT: MonoFont = FONT_10X20;
const SCREEN_TYPE: Screen = Screen::ScreenSaver;
const SCREENSAVER_TIMEOUT: Duration = Duration::from_secs(600);

struct BounceAnimation {
    bounding_box: Rectangle,
}

impl BounceAnimation {
    pub fn new(bounding_box: Rectangle) -> Self {
        Self { bounding_box }
    }

    fn offset(&self, obj_size: Size) -> Point {
        let ticks = SystemTime::UNIX_EPOCH
            .elapsed()
            .map(|t| t.as_millis() / 100)
            .unwrap_or(0);

        let range_x = if self.bounding_box.size.width > obj_size.width {
            self.bounding_box.size.width - obj_size.width
        } else {
            1
        };

        let range_y = if self.bounding_box.size.height > obj_size.height {
            self.bounding_box.size.height - obj_size.height
        } else {
            1
        };

        let bx: i32 = (ticks % (2 * (range_x as u128))).try_into().unwrap_or(0);
        let by: i32 = (ticks % (2 * (range_y as u128))).try_into().unwrap_or(0);

        let range_x: i32 = range_x.try_into().unwrap_or(0);
        let range_y: i32 = range_y.try_into().unwrap_or(0);

        Point::new(
            (bx - range_x).abs() + self.bounding_box.top_left.x,
            (by - range_y).abs() + self.bounding_box.top_left.y,
        )
    }

    pub fn bounce<O: Transform + Dimensions>(&self, obj: O) -> O {
        let obj_size = obj.bounding_box().size;
        obj.translate(self.offset(obj_size))
    }
}

pub struct ScreenSaverScreen;

impl ScreenSaverScreen {
    pub fn new(buttons: &Arc<Topic<ButtonEvent>>, screen: &Arc<Topic<Screen>>) -> Self {
        // Activate screensaver if no button is pressed for some time
        let (mut buttons_events, _) = buttons.clone().subscribe_unbounded();
        let screen_task = screen.clone();
        spawn(async move {
            loop {
                let ev = timeout(SCREENSAVER_TIMEOUT, buttons_events.next()).await;
                let activate_screensaver = match ev {
                    Ok(None) => break,
                    Ok(Some(_)) => false,
                    Err(_) => true,
                };

                if activate_screensaver {
                    screen_task.modify(|screen| {
                        screen.and_then(|s| {
                            if s.use_screensaver() {
                                Some(Screen::ScreenSaver)
                            } else {
                                None
                            }
                        })
                    });
                }
            }
        });

        Self
    }
}

struct Active {
    widgets: WidgetContainer,
    locator: Arc<Topic<bool>>,
    screen: Arc<Topic<Screen>>,
}

impl ActivatableScreen for ScreenSaverScreen {
    fn my_type(&self) -> Screen {
        SCREEN_TYPE
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let hostname = ui.res.network.hostname.clone();
        let bounce = BounceAnimation::new(Rectangle::with_corners(
            Point::new(0, 8),
            Point::new(230, 240),
        ));

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| DynamicWidget::locator(ui.locator_dance.clone(), display));

        widgets.push(|display| {
            DynamicWidget::new(
                ui.res.adc.time.clone(),
                display,
                Box::new(move |_, target| {
                    let ui_text_style: MonoTextStyle<BinaryColor> =
                        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                    let hostname = hostname.try_get().unwrap_or_default();
                    let text = Text::new(&hostname, Point::new(0, 0), ui_text_style);
                    let text = bounce.bounce(text);

                    text.draw(target).unwrap();

                    Some(text.bounding_box())
                }),
            )
        });

        let locator = ui.locator.clone();
        let screen = ui.screen.clone();

        let active = Active {
            widgets,
            locator,
            screen,
        };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    async fn deactivate(mut self: Box<Self>) -> Display {
        self.widgets.destroy().await
    }

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen => self.screen.set(SCREEN_TYPE.next()),
            InputEvent::ToggleAction(_) => {}
            InputEvent::PerformAction(_) => self.locator.toggle(false),
        }
    }
}
