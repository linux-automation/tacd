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

use embedded_graphics::{
    mono_font::MonoTextStyle, pixelcolor::BinaryColor, prelude::*, text::Text,
};

use super::buttons::*;
use super::widgets::*;
use super::{draw_border, row_anchor, ActivatableScreen, ActiveScreen, Display, Screen, Ui};
use crate::broker::{Native, SubscriptionHandle, Topic};
use crate::measurement::Measurement;

const SCREEN_TYPE: Screen = Screen::Usb;
const CURRENT_LIMIT_PER_PORT: f32 = 0.5;
const CURRENT_LIMIT_TOTAL: f32 = 0.7;
const OFFSET_INDICATOR: Point = Point::new(92, -10);
const OFFSET_BAR: Point = Point::new(122, -14);
const WIDTH_BAR: u32 = 90;
const HEIGHT_BAR: u32 = 18;

pub struct UsbScreen {
    highlighted: Arc<Topic<u8>>,
}

impl UsbScreen {
    pub fn new() -> Self {
        Self {
            highlighted: Topic::anonymous(Some(0)),
        }
    }
}

struct Active {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: SubscriptionHandle<ButtonEvent, Native>,
}

impl ActivatableScreen for UsbScreen {
    fn my_type(&self) -> Screen {
        SCREEN_TYPE
    }

    fn activate(&mut self, ui: &Ui, display: Arc<Display>) -> Box<dyn ActiveScreen> {
        draw_border("USB Host", SCREEN_TYPE, &display);

        let mut widgets: Vec<Box<dyn AnyWidget>> = Vec::new();

        widgets.push(Box::new(DynamicWidget::locator(
            ui.locator_dance.clone(),
            display.clone(),
        )));

        let ports = [
            (
                0,
                "Port 1",
                &ui.res.usb_hub.port1.powered,
                &ui.res.adc.usb_host1_curr.topic,
            ),
            (
                1,
                "Port 2",
                &ui.res.usb_hub.port2.powered,
                &ui.res.adc.usb_host2_curr.topic,
            ),
            (
                2,
                "Port 3",
                &ui.res.usb_hub.port3.powered,
                &ui.res.adc.usb_host3_curr.topic,
            ),
        ];

        display.with_lock(|target| {
            let ui_text_style: MonoTextStyle<BinaryColor> =
                MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

            Text::new("Total", row_anchor(0), ui_text_style)
                .draw(target)
                .unwrap();
        });

        widgets.push(Box::new(DynamicWidget::bar(
            ui.res.adc.usb_host_curr.topic.clone(),
            display.clone(),
            row_anchor(0) + OFFSET_BAR,
            WIDTH_BAR,
            HEIGHT_BAR,
            Box::new(|meas: &Measurement| meas.value / CURRENT_LIMIT_TOTAL),
        )));

        for (idx, name, status, current) in ports {
            let anchor_text = row_anchor(idx + 2);
            let anchor_indicator = anchor_text + OFFSET_INDICATOR;
            let anchor_bar = anchor_text + OFFSET_BAR;

            widgets.push(Box::new(DynamicWidget::text(
                self.highlighted.clone(),
                display.clone(),
                anchor_text,
                Box::new(move |highlight: &u8| {
                    format!("{} {}", if *highlight == idx { ">" } else { " " }, name,)
                }),
            )));

            widgets.push(Box::new(DynamicWidget::indicator(
                status.clone(),
                display.clone(),
                anchor_indicator,
                Box::new(|state: &bool| match *state {
                    true => IndicatorState::On,
                    false => IndicatorState::Off,
                }),
            )));

            widgets.push(Box::new(DynamicWidget::bar(
                current.clone(),
                display.clone(),
                anchor_bar,
                WIDTH_BAR,
                HEIGHT_BAR,
                Box::new(|meas: &Measurement| meas.value / CURRENT_LIMIT_PER_PORT),
            )));
        }

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let port_enables = [
            ui.res.usb_hub.port1.powered.clone(),
            ui.res.usb_hub.port2.powered.clone(),
            ui.res.usb_hub.port3.powered.clone(),
        ];
        let port_highlight = self.highlighted.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                let highlighted = port_highlight.get().await;
                let port = &port_enables[highlighted as usize];

                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                        src: _,
                    } => {
                        port.toggle(true);
                    }
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                        src: _,
                    } => {
                        port_highlight.set((highlighted + 1) % 3);
                    }
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                        src: _,
                    } => screen.set(SCREEN_TYPE.next()),
                    ButtonEvent::Press { btn: _, src: _ } => {}
                }
            }
        });

        let active = Active {
            widgets,
            buttons_handle,
        };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    async fn deactivate(mut self: Box<Self>) {
        self.buttons_handle.unsubscribe();

        for mut widget in self.widgets.into_iter() {
            widget.unmount().await
        }
    }
}
