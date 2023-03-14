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
    mono_font::{ascii::FONT_6X9, MonoFont, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};

use super::buttons::*;
use super::widgets::*;
use super::{MountableScreen, Screen, Ui};

use crate::broker::{Native, SubscriptionHandle, Topic};

const UI_TEXT_FONT: MonoFont = FONT_6X9;
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

pub struct ScreenSaverScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl ScreenSaverScreen {
    pub fn new(buttons: &Arc<Topic<ButtonEvent>>, screen: &Arc<Topic<Screen>>) -> Self {
        // Activate screensaver if no button is pressed for some time
        let buttons_task = buttons.clone();
        let screen_task = screen.clone();
        spawn(async move {
            let (mut buttons_events, _) = buttons_task.subscribe_unbounded().await;

            loop {
                let ev = timeout(SCREENSAVER_TIMEOUT, buttons_events.next()).await;
                let activate_screensaver = match ev {
                    Ok(None) => break,
                    Ok(Some(_)) => false,
                    Err(_) => true,
                };

                if activate_screensaver {
                    screen_task
                        .modify(|screen| {
                            screen.and_then(|s| {
                                if s.use_screensaver() {
                                    Some(Screen::ScreenSaver)
                                } else {
                                    None
                                }
                            })
                        })
                        .await;
                }
            }
        });

        Self {
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for ScreenSaverScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        let hostname = ui.res.network.hostname.get().await;
        let bounce = BounceAnimation::new(Rectangle::with_corners(
            Point::new(0, 8),
            Point::new(118, 64),
        ));

        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::new(
                ui.res.adc.time.clone(),
                ui.draw_target.clone(),
                Box::new(move |_, target| {
                    let ui_text_style: MonoTextStyle<BinaryColor> =
                        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                    let text = Text::new(&hostname, Point::new(0, 0), ui_text_style);
                    let text = bounce.bounce(text);
                    text.draw(target).unwrap();

                    Some(text.bounding_box())
                }),
            )
            .await,
        ));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let locator = ui.locator.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: _,
                    } => locator.modify(|prev| Some(!prev.unwrap_or(false))).await,
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                    } => screen.set(SCREEN_TYPE.next()).await,
                    _ => {}
                }
            }
        });

        self.buttons_handle = Some(buttons_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.buttons_handle.take() {
            handle.unsubscribe().await;
        }

        for mut widget in self.widgets.drain(..) {
            widget.unmount().await
        }
    }
}
