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

use crate::broker::{Native, SubscriptionHandle, Topic};

use super::buttons::*;
use super::widgets::*;
use super::{draw_border, MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::Uart;

pub struct UartScreen {
    highlighted: Arc<Topic<u8>>,
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl UartScreen {
    pub fn new() -> Self {
        Self {
            highlighted: Topic::anonymous(Some(0)),
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for UartScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("DUT UART", SCREEN_TYPE, &ui.draw_target).await;

        self.widgets.push(Box::new(DynamicWidget::locator(
            ui.locator_dance.clone(),
            ui.draw_target.clone(),
        )));

        let ports = [
            (0, "UART RX EN", 52, &ui.res.dig_io.uart_rx_en),
            (1, "UART TX EN", 72, &ui.res.dig_io.uart_tx_en),
        ];

        for (idx, name, y, status) in ports {
            self.widgets.push(Box::new(DynamicWidget::text(
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
            )));

            self.widgets.push(Box::new(DynamicWidget::indicator(
                status.clone(),
                ui.draw_target.clone(),
                Point::new(160, y - 10),
                Box::new(|state: &bool| match *state {
                    true => IndicatorState::On,
                    false => IndicatorState::Off,
                }),
            )));
        }

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let dir_enables = [
            ui.res.dig_io.uart_rx_en.clone(),
            ui.res.dig_io.uart_tx_en.clone(),
        ];
        let dir_highlight = self.highlighted.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                let highlighted = dir_highlight.get().await;
                let port = &dir_enables[highlighted as usize];

                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                    } => port.modify(|prev| Some(!prev.unwrap_or(false))),
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                    } => {
                        dir_highlight.set((highlighted + 1) % 2);
                    }
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                    } => screen.set(SCREEN_TYPE.next()),
                    ButtonEvent::Press { btn: _ } => {}
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
