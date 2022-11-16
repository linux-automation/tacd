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

use crate::adc::Measurement;
use crate::broker::{Native, SubscriptionHandle};
use crate::dut_power::{OutputRequest, OutputState};

use super::widgets::*;
use super::{draw_border, ButtonEvent, MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::DutPower;

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

        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.adc.pwr_volt.topic.clone(),
                ui.draw_target.clone(),
                Point::new(8, 52),
                Box::new(|meas: &Measurement| format!("V: {:-6.3}V", meas.value)),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::bar(
                ui.res.adc.pwr_volt.topic.clone(),
                ui.draw_target.clone(),
                Point::new(120, 52 - 14),
                100,
                18,
                Box::new(|meas: &Measurement| meas.value / 48.0),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.adc.pwr_curr.topic.clone(),
                ui.draw_target.clone(),
                Point::new(8, 72),
                Box::new(|meas: &Measurement| format!("I: {:-6.3}A", meas.value)),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::bar(
                ui.res.adc.pwr_curr.topic.clone(),
                ui.draw_target.clone(),
                Point::new(120, 72 - 14),
                100,
                18,
                Box::new(|meas: &Measurement| meas.value / 48.0),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.dut_pwr.state.clone(),
                ui.draw_target.clone(),
                Point::new(8, 92),
                Box::new(|state: &OutputState| match state {
                    OutputState::On => "On".into(),
                    OutputState::Off => "Off".into(),
                    OutputState::OffFloating => "Off (Float.)".into(),
                    OutputState::InvertedPolarity => "Inv. Pol.".into(),
                    OutputState::OverCurrent => "Ov. Curr.".into(),
                    OutputState::OverVoltage => "Ov. Volt.".into(),
                    OutputState::RealtimeViolation => "Rt Err.".into(),
                }),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::indicator(
                ui.res.dut_pwr.state.clone(),
                ui.draw_target.clone(),
                Point::new(120, 92 - 10),
                Box::new(|state: &OutputState| match state {
                    OutputState::On => IndicatorState::On,
                    OutputState::Off | OutputState::OffFloating => IndicatorState::Off,
                    _ => IndicatorState::Error,
                }),
            )
            .await,
        ));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let power_state = ui.res.dut_pwr.state.clone();
        let power_request = ui.res.dut_pwr.request.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                if let ButtonEvent::ButtonOne(_) = *ev {
                    let state = *power_state.get().await;

                    let req = match state {
                        OutputState::On => OutputRequest::Off,
                        _ => OutputRequest::On,
                    };

                    power_request.set(req).await;
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
