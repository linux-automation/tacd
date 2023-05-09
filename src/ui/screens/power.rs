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
use embedded_graphics::prelude::*;

use super::widgets::*;
use super::{
    draw_border, row_anchor, ActivatableScreen, ActiveScreen, Display, InputEvent, NormalScreen,
    Screen, Ui,
};
use crate::broker::Topic;
use crate::dut_power::{OutputRequest, OutputState};
use crate::measurement::Measurement;

const SCREEN_TYPE: NormalScreen = NormalScreen::DutPower;
const CURRENT_LIMIT: f32 = 5.0;
const VOLTAGE_LIMIT: f32 = 48.0;
const OFFSET_INDICATOR: Point = Point::new(155, -10);
const OFFSET_BAR: Point = Point::new(112, -14);
const WIDTH_BAR: u32 = 100;
const HEIGHT_BAR: u32 = 18;

pub struct PowerScreen;

impl PowerScreen {
    pub fn new() -> Self {
        Self
    }
}

struct Active {
    widgets: WidgetContainer,
    power_state: Arc<Topic<OutputState>>,
    power_request: Arc<Topic<OutputRequest>>,
}

impl ActivatableScreen for PowerScreen {
    fn my_type(&self) -> Screen {
        Screen::Normal(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        draw_border("DUT Power", SCREEN_TYPE, &display);

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| DynamicWidget::locator(ui.locator_dance.clone(), display));

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.adc.pwr_volt.topic.clone(),
                display,
                row_anchor(0),
                Box::new(|meas: &Measurement| format!("V: {:-6.3}V", meas.value)),
            )
        });

        widgets.push(|display| {
            DynamicWidget::bar(
                ui.res.adc.pwr_volt.topic.clone(),
                display,
                row_anchor(0) + OFFSET_BAR,
                WIDTH_BAR,
                HEIGHT_BAR,
                Box::new(|meas: &Measurement| meas.value / VOLTAGE_LIMIT),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.adc.pwr_curr.topic.clone(),
                display,
                row_anchor(1),
                Box::new(|meas: &Measurement| format!("I: {:-6.3}A", meas.value)),
            )
        });

        widgets.push(|display| {
            DynamicWidget::bar(
                ui.res.adc.pwr_curr.topic.clone(),
                display,
                row_anchor(1) + OFFSET_BAR,
                WIDTH_BAR,
                HEIGHT_BAR,
                Box::new(|meas: &Measurement| meas.value / CURRENT_LIMIT),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.dut_pwr.state.clone(),
                display,
                row_anchor(3),
                Box::new(|state: &OutputState| match state {
                    OutputState::On => "> On".into(),
                    OutputState::Off => "> Off".into(),
                    OutputState::Changing => "> Changing".into(),
                    OutputState::OffFloating => "> Off (Float.)".into(),
                    OutputState::InvertedPolarity => "> Inv. Pol.".into(),
                    OutputState::OverCurrent => "> Ov. Curr.".into(),
                    OutputState::OverVoltage => "> Ov. Volt.".into(),
                    OutputState::RealtimeViolation => "> Rt Err.".into(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::indicator(
                ui.res.dut_pwr.state.clone(),
                display,
                row_anchor(3) + OFFSET_INDICATOR,
                Box::new(|state: &OutputState| match state {
                    OutputState::On => IndicatorState::On,
                    OutputState::Off | OutputState::OffFloating => IndicatorState::Off,
                    OutputState::Changing => IndicatorState::Unkown,
                    _ => IndicatorState::Error,
                }),
            )
        });

        let power_state = ui.res.dut_pwr.state.clone();
        let power_request = ui.res.dut_pwr.request.clone();

        let active = Active {
            widgets,
            power_state,
            power_request,
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
        match ev {
            InputEvent::NextScreen | InputEvent::ToggleAction(_) => {}
            InputEvent::PerformAction(_) => {
                let req = match self.power_state.try_get() {
                    Some(OutputState::On) => OutputRequest::Off,
                    _ => OutputRequest::On,
                };

                self.power_request.set(req);
            }
        }
    }
}
