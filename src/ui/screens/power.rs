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
use async_std::task::spawn;
use async_trait::async_trait;

use embedded_graphics::prelude::*;

use super::buttons::*;
use super::widgets::*;
use super::{draw_border, row_anchor, MountableScreen, Screen, Ui};
use crate::broker::{Native, SubscriptionHandle};
use crate::dut_power::{OutputRequest, OutputState};
use crate::measurement::Measurement;

const SCREEN_TYPE: Screen = Screen::DutPower;
const CURRENT_LIMIT: f32 = 5.0;
const VOLTAGE_LIMIT: f32 = 48.0;
const OFFSET_INDICATOR: Point = Point::new(155, -10);
const OFFSET_BAR: Point = Point::new(112, -14);
const WIDTH_BAR: u32 = 100;
const HEIGHT_BAR: u32 = 18;

pub struct PowerScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl PowerScreen {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for PowerScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("DUT Power", SCREEN_TYPE, &ui.draw_target).await;

        self.widgets.push(Box::new(DynamicWidget::locator(
            ui.locator_dance.clone(),
            ui.draw_target.clone(),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.adc.pwr_volt.topic.clone(),
            ui.draw_target.clone(),
            row_anchor(0),
            Box::new(|meas: &Measurement| format!("V: {:-6.3}V", meas.value)),
        )));

        self.widgets.push(Box::new(DynamicWidget::bar(
            ui.res.adc.pwr_volt.topic.clone(),
            ui.draw_target.clone(),
            row_anchor(0) + OFFSET_BAR,
            WIDTH_BAR,
            HEIGHT_BAR,
            Box::new(|meas: &Measurement| meas.value / VOLTAGE_LIMIT),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.adc.pwr_curr.topic.clone(),
            ui.draw_target.clone(),
            row_anchor(1),
            Box::new(|meas: &Measurement| format!("I: {:-6.3}A", meas.value)),
        )));

        self.widgets.push(Box::new(DynamicWidget::bar(
            ui.res.adc.pwr_curr.topic.clone(),
            ui.draw_target.clone(),
            row_anchor(1) + OFFSET_BAR,
            WIDTH_BAR,
            HEIGHT_BAR,
            Box::new(|meas: &Measurement| meas.value / CURRENT_LIMIT),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.dut_pwr.state.clone(),
            ui.draw_target.clone(),
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
        )));

        self.widgets.push(Box::new(DynamicWidget::indicator(
            ui.res.dut_pwr.state.clone(),
            ui.draw_target.clone(),
            row_anchor(3) + OFFSET_INDICATOR,
            Box::new(|state: &OutputState| match state {
                OutputState::On => IndicatorState::On,
                OutputState::Off | OutputState::OffFloating => IndicatorState::Off,
                OutputState::Changing => IndicatorState::Unkown,
                _ => IndicatorState::Error,
            }),
        )));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let power_state = ui.res.dut_pwr.state.clone();
        let power_request = ui.res.dut_pwr.request.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                        src: _,
                    } => {
                        let req = match power_state.get().await {
                            OutputState::On => OutputRequest::Off,
                            _ => OutputRequest::On,
                        };

                        power_request.set(req);
                    }
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                        src: _,
                    } => screen.set(SCREEN_TYPE.next()),
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                        src: _,
                    } => {}
                    ButtonEvent::Press { btn: _, src: _ } => {}
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
