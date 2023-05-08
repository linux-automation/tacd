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

use std::sync::Arc;

use async_std::prelude::*;
use async_std::task::spawn;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::buttons::*;
use super::widgets::*;
use super::{draw_border, row_anchor, Display, MountableScreen, Screen, Ui};
use crate::broker::{Native, SubscriptionHandle, Topic};
use crate::dbus::networkmanager::LinkInfo;
use crate::measurement::Measurement;

const SCREEN_TYPE: Screen = Screen::System;

#[derive(Serialize, Deserialize, Clone, Copy)]
enum Action {
    Reboot,
    Help,
    SetupMode,
}

impl Action {
    fn next(&self) -> Self {
        match self {
            Self::Reboot => Self::Help,
            Self::Help => Self::SetupMode,
            Self::SetupMode => Self::Reboot,
        }
    }
}

pub struct SystemScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl SystemScreen {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for SystemScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui, display: Arc<Display>) {
        draw_border("System Status", SCREEN_TYPE, &display).await;

        let highlighted = Topic::anonymous(Some(Action::Reboot));

        self.widgets.push(Box::new(DynamicWidget::locator(
            ui.locator_dance.clone(),
            display.clone(),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.temperatures.soc_temperature.clone(),
            display.clone(),
            row_anchor(0),
            Box::new(|meas: &Measurement| format!("SoC:    {:.0}C", meas.value)),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.network.uplink_interface.clone(),
            display.clone(),
            row_anchor(1),
            Box::new(|info: &LinkInfo| match info.carrier {
                true => format!("Uplink: {}MBit/s", info.speed),
                false => "Uplink: Down".to_string(),
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.network.dut_interface.clone(),
            display.clone(),
            row_anchor(2),
            Box::new(|info: &LinkInfo| match info.carrier {
                true => format!("DUT:    {}MBit/s", info.speed),
                false => "DUT:    Down".to_string(),
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            ui.res.network.bridge_interface.clone(),
            display.clone(),
            row_anchor(3),
            Box::new(|ips: &Vec<String>| {
                let ip = ips.get(0).map(|s| s.as_str()).unwrap_or("-");
                format!("IP:     {}", ip)
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            highlighted.clone(),
            display.clone(),
            row_anchor(5),
            Box::new(|action| match action {
                Action::Reboot => "> Reboot".into(),
                _ => "  Reboot".into(),
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            highlighted.clone(),
            display.clone(),
            row_anchor(6),
            Box::new(|action| match action {
                Action::Help => "> Help".into(),
                _ => "  Help".into(),
            }),
        )));

        self.widgets.push(Box::new(DynamicWidget::text(
            highlighted.clone(),
            display.clone(),
            row_anchor(7),
            Box::new(|action| match action {
                Action::SetupMode => "> Setup Mode".into(),
                _ => "  Setup Mode".into(),
            }),
        )));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded();
        let setup_mode = ui.res.setup_mode.setup_mode.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                let action = highlighted.get().await;

                match ev {
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: _,
                        src: Source::Web,
                    } => {
                        /* Only allow upper button interaction (going to the next screen)
                         * for inputs on the web.
                         * Triggering Reboots is possible via the API, so we do not have to
                         * protect against that and opening the help text is harmless as well,
                         * but we could think of an attacker that tricks a local user into
                         * long pressing the lower button right when the attacker goes to the
                         * "Setup Mode" entry in the menu so that they can deploy new keys.
                         * Prevent that by disabling navigation altogether. */
                    }
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Long,
                        src: Source::Local,
                    } => match action {
                        Action::Reboot => screen.set(Screen::RebootConfirm),
                        Action::Help => screen.set(Screen::Help),
                        Action::SetupMode => setup_mode.set(true),
                    },
                    ButtonEvent::Release {
                        btn: Button::Lower,
                        dur: PressDuration::Short,
                        src: Source::Local,
                    } => highlighted.set(action.next()),
                    ButtonEvent::Release {
                        btn: Button::Upper,
                        dur: _,
                        src: _,
                    } => {
                        screen.set(SCREEN_TYPE.next());
                    }
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
