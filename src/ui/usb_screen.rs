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

use crate::adc::Measurement;
use crate::broker::{BrokerBuilder, Native, SubscriptionHandle, Topic};

use super::widgets::*;
use super::{draw_border, ButtonEvent, MountableScreen, Screen, Ui, LONG_PRESS};

const SCREEN_TYPE: Screen = Screen::Usb;

pub struct UsbScreen {
    highlighted: Arc<Topic<u8>>,
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl UsbScreen {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        Self {
            highlighted: bb.topic_hidden(Some(0)),
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for UsbScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("USB Host", SCREEN_TYPE, &ui.draw_target).await;

        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        let ports = [
            (
                0,
                "Port 1",
                52,
                &ui.res.usb_hub.port1.powered,
                &ui.res.adc.usb_host1_curr.topic,
            ),
            (
                1,
                "Port 2",
                72,
                &ui.res.usb_hub.port2.powered,
                &ui.res.adc.usb_host2_curr.topic,
            ),
            (
                2,
                "Port 3",
                92,
                &ui.res.usb_hub.port3.powered,
                &ui.res.adc.usb_host3_curr.topic,
            ),
        ];

        for (idx, name, y, status, current) in ports {
            self.widgets.push(Box::new(
                DynamicWidget::text(
                    self.highlighted.clone(),
                    ui.draw_target.clone(),
                    Point::new(8, y),
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
                    Point::new(100, y - 10),
                    Box::new(|state: &bool| match *state {
                        true => IndicatorState::On,
                        false => IndicatorState::Off,
                    }),
                )
                .await,
            ));

            self.widgets.push(Box::new(
                DynamicWidget::bar(
                    current.clone(),
                    ui.draw_target.clone(),
                    Point::new(130, y - 14),
                    90,
                    18,
                    Box::new(|meas: &Measurement| meas.value / 0.5),
                )
                .await,
            ));
        }

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let port_enables = [
            ui.res.usb_hub.port1.powered.clone(),
            ui.res.usb_hub.port2.powered.clone(),
            ui.res.usb_hub.port3.powered.clone(),
        ];
        let port_highlight = self.highlighted.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                let highlighted = *port_highlight.get().await;

                if let ButtonEvent::ButtonOne(dur) = *ev {
                    if dur > LONG_PRESS {
                        let port = &port_enables[highlighted as usize];
                        port.modify(|prev| {
                            Some(Arc::new(!prev.as_deref().copied().unwrap_or(true)))
                        })
                        .await;
                    } else {
                        port_highlight.set((highlighted + 1) % 3).await;
                    }
                }

                if let ButtonEvent::ButtonTwo(_) = *ev {
                    screen.set(SCREEN_TYPE.next()).await
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
