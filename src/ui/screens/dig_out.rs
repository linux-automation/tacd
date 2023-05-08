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
use super::{draw_border, row_anchor, MountableScreen, Screen, Ui};
use crate::broker::{Native, SubscriptionHandle, Topic};
use crate::measurement::Measurement;

const SCREEN_TYPE: Screen = Screen::DigOut;
const VOLTAGE_MAX: f32 = 5.0;
const OFFSET_INDICATOR: Point = Point::new(170, -10);
const OFFSET_BAR: Point = Point::new(140, -14);
const WIDTH_BAR: u32 = 72;
const HEIGHT_BAR: u32 = 18;

pub struct DigOutScreen {
    highlighted: Arc<Topic<u8>>,
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl DigOutScreen {
    pub fn new() -> Self {
        Self {
            highlighted: Topic::anonymous(Some(0)),
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

        self.widgets.push(Box::new(DynamicWidget::locator(
            ui.locator_dance.clone(),
            ui.draw_target.clone(),
        )));

        let ports = [
            (
                0,
                "OUT 0:",
                &ui.res.dig_io.out_0,
                &ui.res.adc.out0_volt.topic,
            ),
            (
                1,
                "OUT 1:",
                &ui.res.dig_io.out_1,
                &ui.res.adc.out1_volt.topic,
            ),
        ];

        for (idx, name, status, voltage) in ports {
            let anchor_name = row_anchor(idx * 4);
            let anchor_assert = row_anchor(idx * 4 + 1);
            let anchor_indicator = anchor_assert + OFFSET_INDICATOR;

            let anchor_voltage = row_anchor(idx * 4 + 2);
            let anchor_bar = anchor_voltage + OFFSET_BAR;

            {
                let mut draw_target = ui.draw_target.lock().await;

                let ui_text_style: MonoTextStyle<BinaryColor> =
                    MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                Text::new(name, anchor_name, ui_text_style)
                    .draw(&mut *draw_target)
                    .unwrap();
            }

            self.widgets.push(Box::new(DynamicWidget::text(
                self.highlighted.clone(),
                ui.draw_target.clone(),
                anchor_assert,
                Box::new(move |highlight: &u8| {
                    if *highlight == idx {
                        "> Asserted:".into()
                    } else {
                        "  Asserted:".into()
                    }
                }),
            )));

            self.widgets.push(Box::new(DynamicWidget::indicator(
                status.clone(),
                ui.draw_target.clone(),
                anchor_indicator,
                Box::new(|state: &bool| match *state {
                    true => IndicatorState::On,
                    false => IndicatorState::Off,
                }),
            )));

            self.widgets.push(Box::new(DynamicWidget::text(
                voltage.clone(),
                ui.draw_target.clone(),
                anchor_voltage,
                Box::new(|meas: &Measurement| format!("  Volt: {:>4.1}V", meas.value)),
            )));

            self.widgets.push(Box::new(DynamicWidget::bar(
                voltage.clone(),
                ui.draw_target.clone(),
                anchor_bar,
                WIDTH_BAR,
                HEIGHT_BAR,
                Box::new(|meas: &Measurement| meas.value.abs() / VOLTAGE_MAX),
            )));
        }

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let port_enables = [ui.res.dig_io.out_0.clone(), ui.res.dig_io.out_1.clone()];
        let port_highlight = self.highlighted.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                let highlighted = port_highlight.get().await;

                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                        src: _,
                    } => {
                        port_enables[highlighted as usize].toggle(true);
                    }
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                        src: _,
                    } => {
                        port_highlight.set((highlighted + 1) % 2);
                    }
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                        src: _,
                    } => {
                        screen.set(SCREEN_TYPE.next());
                    }
                    _ => {}
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
