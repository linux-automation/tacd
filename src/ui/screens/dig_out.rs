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

const SCREEN_TYPE: NormalScreen = NormalScreen::DigOut;
const VOLTAGE_MAX: f32 = 5.0;
const OFFSET_INDICATOR: Point = Point::new(170, -10);
const OFFSET_BAR: Point = Point::new(140, -14);
const WIDTH_BAR: u32 = 72;
const HEIGHT_BAR: u32 = 18;

pub struct DigOutScreen {
    highlighted: Arc<Topic<usize>>,
}

impl DigOutScreen {
    pub fn new() -> Self {
        Self {
            highlighted: Topic::anonymous(Some(0)),
        }
    }
}

struct Active {
    widgets: WidgetContainer,
    port_enables: [Arc<Topic<bool>>; 2],
    highlighted: Arc<Topic<usize>>,
}

impl ActivatableScreen for DigOutScreen {
    fn my_type(&self) -> Screen {
        Screen::Normal(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        draw_border("Digital Out", SCREEN_TYPE, &display);

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

        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            for (idx, name, _, _) in ports {
                let anchor_name = row_anchor(idx * 4);

                Text::new(name, anchor_name, ui_text_style)
                    .draw(target)
                    .unwrap();
            }
        });

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| DynamicWidget::locator(ui.locator_dance.clone(), display));

        for (idx, _, status, voltage) in ports {
            let anchor_assert = row_anchor(idx * 4 + 1);
            let anchor_indicator = anchor_assert + OFFSET_INDICATOR;

            let anchor_voltage = row_anchor(idx * 4 + 2);
            let anchor_bar = anchor_voltage + OFFSET_BAR;

            widgets.push(|display| {
                DynamicWidget::text(
                    self.highlighted.clone(),
                    display,
                    anchor_assert,
                    Box::new(move |highlight| {
                        if *highlight == (idx as usize) {
                            "> Asserted:".into()
                        } else {
                            "  Asserted:".into()
                        }
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
                DynamicWidget::text(
                    voltage.clone(),
                    display,
                    anchor_voltage,
                    Box::new(|meas: &Measurement| format!("  Volt: {:>4.1}V", meas.value)),
                )
            });

            widgets.push(|display| {
                DynamicWidget::bar(
                    voltage.clone(),
                    display,
                    anchor_bar,
                    WIDTH_BAR,
                    HEIGHT_BAR,
                    Box::new(|meas: &Measurement| meas.value.abs() / VOLTAGE_MAX),
                )
            });
        }

        let port_enables = [ui.res.dig_io.out_0.clone(), ui.res.dig_io.out_1.clone()];
        let highlighted = self.highlighted.clone();

        let active = Active {
            widgets,
            port_enables,
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
            InputEvent::PerformAction(_) => {
                self.port_enables[highlighted].toggle(true);
            }
        }
    }
}
