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

use embedded_graphics::{
    mono_font::MonoTextStyle, pixelcolor::BinaryColor, prelude::*, text::Text,
};

use crate::broker::{Native, SubscriptionHandle};
use crate::iobus::{LSSState, Nodes, ServerInfo};

use super::buttons::*;
use super::widgets::*;
use super::{draw_border, row_anchor, MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::IoBus;
const OFFSET_INDICATOR: Point = Point::new(180, -10);

pub struct IoBusScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl IoBusScreen {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for IoBusScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("IOBus", SCREEN_TYPE, &ui.display).await;

        ui.display.with_lock(|target| {
            let ui_text_style: MonoTextStyle<BinaryColor> =
                MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

            Text::new("CAN Status:", row_anchor(0), ui_text_style)
                .draw(target)
                .unwrap();

            Text::new("LSS Scan Status:", row_anchor(1), ui_text_style)
                .draw(target)
                .unwrap();

            Text::new("Power Fault:", row_anchor(2), ui_text_style)
                .draw(target)
                .unwrap();

            Text::new("> Power On:", row_anchor(5), ui_text_style)
                .draw(target)
                .unwrap();
        });

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.iobus.nodes.clone(),
            ui.display.clone(),
            row_anchor(3),
            Box::new(move |nodes: &Nodes| format!("Connected Nodes:  {}", nodes.result.len())),
        )));

        self.widgets.push(Box::new(DynamicWidget::locator(
            ui.locator_dance.clone(),
            ui.display.clone(),
        )));

        self.widgets.push(Box::new(DynamicWidget::indicator(
            ui.res.iobus.server_info.clone(),
            ui.display.clone(),
            row_anchor(0) + OFFSET_INDICATOR,
            Box::new(|info: &ServerInfo| match info.can_tx_error {
                false => IndicatorState::On,
                true => IndicatorState::Error,
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::indicator(
            ui.res.iobus.server_info.clone(),
            ui.display.clone(),
            row_anchor(1) + OFFSET_INDICATOR,
            Box::new(|info: &ServerInfo| match info.lss_state {
                LSSState::Scanning => IndicatorState::On,
                LSSState::Idle => IndicatorState::Off,
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::indicator(
            ui.res.dig_io.iobus_flt_fb.clone(),
            ui.display.clone(),
            row_anchor(2) + OFFSET_INDICATOR,
            Box::new(|state: &bool| match *state {
                true => IndicatorState::Error,
                false => IndicatorState::Off,
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::indicator(
            ui.res.regulators.iobus_pwr_en.clone(),
            ui.display.clone(),
            row_anchor(5) + OFFSET_INDICATOR,
            Box::new(|state: &bool| match *state {
                true => IndicatorState::On,
                false => IndicatorState::Off,
            }),
        )));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let iobus_pwr_en = ui.res.regulators.iobus_pwr_en.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                        src: _,
                    } => iobus_pwr_en.toggle(true),
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
