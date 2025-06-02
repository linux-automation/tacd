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

use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::prelude::*;

use super::widgets::*;
use super::{
    draw_border, ActivatableScreen, ActiveScreen, Display, InputEvent, NormalScreen, Screen, Ui,
};
use crate::broker::Topic;

const SCREEN_TYPE: NormalScreen = NormalScreen::Uart;

pub struct UartScreen {
    highlighted: Arc<Topic<usize>>,
}

impl UartScreen {
    pub fn new() -> Self {
        Self {
            highlighted: Topic::anonymous(Some(0)),
        }
    }
}

struct Active {
    widgets: WidgetContainer,
    dir_enables: [Arc<Topic<bool>>; 2],
    highlighted: Arc<Topic<usize>>,
}

impl ActivatableScreen for UartScreen {
    fn my_type(&self) -> Screen {
        Screen::Normal(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        display.with_lock(|target| {
            draw_border(target, "DUT UART", SCREEN_TYPE);
            draw_button_legend(target, "Action", "Screen")
        });

        let mut widgets = WidgetContainer::new(display);

        let ports = [
            (0, "UART RX EN", 52, &ui.res.dig_io.uart_rx_en),
            (1, "UART TX EN", 72, &ui.res.dig_io.uart_tx_en),
        ];

        for (idx, name, y, status) in ports {
            widgets.push(|display| {
                DynamicWidget::text(
                    self.highlighted.clone(),
                    display,
                    Point::new(8, y),
                    Box::new(move |highlight| {
                        format!("{} {}", if *highlight == idx { ">" } else { " " }, name,)
                    }),
                )
            });

            widgets.push(|display| {
                DynamicWidget::indicator(
                    status.clone(),
                    display,
                    Point::new(160, y - 10),
                    Box::new(|state: &bool| match *state {
                        true => IndicatorState::On,
                        false => IndicatorState::Off,
                    }),
                )
            });
        }

        let dir_enables = [
            ui.res.dig_io.uart_rx_en.clone(),
            ui.res.dig_io.uart_tx_en.clone(),
        ];
        let highlighted = self.highlighted.clone();

        let active = Active {
            widgets,
            dir_enables,
            highlighted,
        };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    fn my_type(&self) -> Screen {
        Screen::Normal(SCREEN_TYPE)
    }

    async fn deactivate(mut self: Box<Self>) -> Display {
        self.widgets.destroy().await
    }

    fn input(&mut self, ev: InputEvent) {
        let highlighted = self.highlighted.try_get().unwrap_or(0);

        match ev {
            InputEvent::NextScreen => {}
            InputEvent::ToggleAction(_) => {
                self.highlighted.set((highlighted + 1) % 2);
            }
            InputEvent::PerformAction(_) => self.dir_enables[highlighted].toggle(false),
        }
    }
}
