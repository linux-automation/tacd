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

use super::buttons::*;
use super::widgets::*;
use super::{draw_border, MountableScreen, Screen, Ui};
use crate::broker::{BrokerBuilder, Native, SubscriptionHandle, Topic};
use crate::measurement::Measurement;

const SCREEN_TYPE: Screen = Screen::DigOut;

pub struct DigOutScreen {
    highlighted: Arc<Topic<u8>>,
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl DigOutScreen {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        Self {
            highlighted: bb.topic_hidden(Some(0)),
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for DigOutScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("Digital Out", SCREEN_TYPE, &ui.draw_target).await;

        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        let ports = [
            (
                0,
                "OUT 0",
                29,
                &ui.res.dig_io.out_0,
                &ui.res.adc.out0_volt.topic,
            ),
            (
                1,
                "OUT 1",
                44,
                &ui.res.dig_io.out_1,
                &ui.res.adc.out1_volt.topic,
            ),
        ];

        for (idx, name, y, status, voltage) in ports {
            self.widgets.push(Box::new(
                DynamicWidget::text(
                    self.highlighted.clone(),
                    ui.draw_target.clone(),
                    Point::new(0, y),
                    Box::new(move |highlight: &u8| {
                        format!(
                            "{} {}",
                            if *highlight as usize == idx { ">" } else { " " },
                            name,
                        )
                    }),
                )
                .await,
            ));

            self.widgets.push(Box::new(
                DynamicWidget::indicator(
                    status.clone(),
                    ui.draw_target.clone(),
                    Point::new(54, y - 7),
                    Box::new(|state: &bool| match *state {
                        true => IndicatorState::On,
                        false => IndicatorState::Off,
                    }),
                )
                .await,
            ));

            self.widgets.push(Box::new(
                DynamicWidget::bar(
                    voltage.clone(),
                    ui.draw_target.clone(),
                    Point::new(70, y - 6),
                    45,
                    7,
                    Box::new(|meas: &Measurement| meas.value.abs() / 5.0),
                )
                .await,
            ));
        }

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let port_enables = [ui.res.dig_io.out_0.clone(), ui.res.dig_io.out_1.clone()];
        let port_highlight = self.highlighted.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                let highlighted = *port_highlight.get().await;

                match *ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                    } => {
                        let port = &port_enables[highlighted as usize];

                        port.modify(|prev| {
                            Some(Arc::new(!prev.as_deref().copied().unwrap_or(true)))
                        })
                        .await;
                    }
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                    } => {
                        port_highlight.set((highlighted + 1) % 2).await;
                    }
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                    } => {
                        screen.set(SCREEN_TYPE.next()).await;
                    }
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
