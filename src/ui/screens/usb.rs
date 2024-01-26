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

use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle, pixelcolor::BinaryColor, prelude::*, text::Text,
};

use super::widgets::*;
use super::{
    draw_border, row_anchor, ActivatableScreen, ActiveScreen, Display, InputEvent, NormalScreen,
    Screen, Ui,
};
use crate::broker::Topic;
use crate::measurement::Measurement;
use crate::usb_hub::{MAX_PORT_CURRENT, MAX_TOTAL_CURRENT};

const SCREEN_TYPE: NormalScreen = NormalScreen::Usb;
const OFFSET_INDICATOR: Point = Point::new(92, -10);
const OFFSET_BAR: Point = Point::new(122, -14);
const WIDTH_BAR: u32 = 90;
const HEIGHT_BAR: u32 = 18;

pub struct UsbScreen {
    highlighted: Arc<Topic<usize>>,
}

impl UsbScreen {
    pub fn new() -> Self {
        Self {
            highlighted: Topic::anonymous(Some(0)),
        }
    }
}

struct Active {
    widgets: WidgetContainer,
    port_requests: [Arc<Topic<bool>>; 3],
    port_states: [Arc<Topic<bool>>; 3],
    highlighted: Arc<Topic<usize>>,
}

impl ActivatableScreen for UsbScreen {
    fn my_type(&self) -> Screen {
        Screen::Normal(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            draw_border(target, "USB Host", SCREEN_TYPE);

            Text::new("Total", row_anchor(0), ui_text_style)
                .draw(target)
                .unwrap();
        });

        let mut widgets = WidgetContainer::new(display);

        let ports = [
            (
                0,
                "Port 1",
                &ui.res.usb_hub.port1.status,
                &ui.res.adc.usb_host1_curr.topic,
            ),
            (
                1,
                "Port 2",
                &ui.res.usb_hub.port2.status,
                &ui.res.adc.usb_host2_curr.topic,
            ),
            (
                2,
                "Port 3",
                &ui.res.usb_hub.port3.status,
                &ui.res.adc.usb_host3_curr.topic,
            ),
        ];

        widgets.push(|display| {
            DynamicWidget::bar(
                ui.res.adc.usb_host_curr.topic.clone(),
                display,
                row_anchor(0) + OFFSET_BAR,
                WIDTH_BAR,
                HEIGHT_BAR,
                Box::new(|meas: &Measurement| meas.value / MAX_TOTAL_CURRENT),
            )
        });

        for (idx, name, status, current) in ports {
            let anchor_text = row_anchor(idx + 2);
            let anchor_indicator = anchor_text + OFFSET_INDICATOR;
            let anchor_bar = anchor_text + OFFSET_BAR;

            widgets.push(|display| {
                DynamicWidget::text(
                    self.highlighted.clone(),
                    display,
                    anchor_text,
                    Box::new(move |highlight| {
                        let hl = *highlight == (idx as usize);
                        format!("{} {}", if hl { ">" } else { " " }, name)
                    }),
                )
            });

            widgets.push(|display| {
                DynamicWidget::indicator(
                    status.clone(),
                    display,
                    anchor_indicator,
                    Box::new(|state: &bool| match *state {
                        true => IndicatorState::On,
                        false => IndicatorState::Off,
                    }),
                )
            });

            widgets.push(|display| {
                DynamicWidget::bar(
                    current.clone(),
                    display,
                    anchor_bar,
                    WIDTH_BAR,
                    HEIGHT_BAR,
                    Box::new(|meas: &Measurement| meas.value / MAX_PORT_CURRENT),
                )
            });
        }

        let port_requests = [
            ui.res.usb_hub.port1.request.clone(),
            ui.res.usb_hub.port2.request.clone(),
            ui.res.usb_hub.port3.request.clone(),
        ];
        let port_states = [
            ui.res.usb_hub.port1.status.clone(),
            ui.res.usb_hub.port2.status.clone(),
            ui.res.usb_hub.port3.status.clone(),
        ];
        let highlighted = self.highlighted.clone();

        let active = Active {
            widgets,
            port_requests,
            port_states,
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
                self.highlighted.set((highlighted + 1) % 3);
            }
            InputEvent::PerformAction(_) => {
                let status = self.port_states[highlighted].try_get().unwrap_or(false);
                self.port_requests[highlighted].set(!status);
            }
        }
    }
}
