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
use serde::{Deserialize, Serialize};

use super::buttons::Source;
use super::widgets::*;
use super::{
    draw_border, row_anchor, ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter,
    Display, InputEvent, NormalScreen, Screen, Ui,
};
use crate::broker::Topic;
use crate::dbus::networkmanager::LinkInfo;
use crate::measurement::Measurement;

const SCREEN_TYPE: NormalScreen = NormalScreen::System;

#[derive(Serialize, Deserialize, Clone, Copy)]
enum Action {
    Reboot,
    Help,
    SetupMode,
    Updates,
}

impl Action {
    fn next(&self) -> Self {
        match self {
            Self::Reboot => Self::Help,
            Self::Help => Self::SetupMode,
            Self::SetupMode => Self::Updates,
            Self::Updates => Self::Reboot,
        }
    }
}

pub struct SystemScreen;

impl SystemScreen {
    pub fn new() -> Self {
        Self
    }
}

struct Active {
    widgets: WidgetContainer,
    setup_mode: Arc<Topic<bool>>,
    highlighted: Arc<Topic<Action>>,
    reboot_message: Arc<Topic<Option<String>>>,
    show_help: Arc<Topic<bool>>,
    alerts: Arc<Topic<AlertList>>,
}

impl ActivatableScreen for SystemScreen {
    fn my_type(&self) -> Screen {
        Screen::Normal(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        display.with_lock(|target| {
            draw_border(target, "System Status", SCREEN_TYPE);
            draw_button_legend(target, "Action", "Screen")
        });

        let mut widgets = WidgetContainer::new(display);
        let highlighted = Topic::anonymous(Some(Action::Reboot));

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.temperatures.soc_temperature.clone(),
                display,
                row_anchor(0),
                Box::new(|meas: &Measurement| format!("SoC: {:.0}C", meas.value)),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.network.uplink_interface.clone(),
                display,
                row_anchor(1),
                Box::new(|info: &LinkInfo| match info.carrier {
                    true => format!("UL:  {}MBit/s", info.speed),
                    false => "UL:  Down".to_string(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.network.dut_interface.clone(),
                display,
                row_anchor(2),
                Box::new(|info: &LinkInfo| match info.carrier {
                    true => format!("DUT: {}MBit/s", info.speed),
                    false => "DUT: Down".to_string(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.network.bridge_interface.clone(),
                display,
                row_anchor(3),
                Box::new(|ips: &Vec<String>| {
                    let ip = ips.first().map(|s| s.as_str()).unwrap_or("-");
                    format!("IP:  {}", ip)
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                highlighted.clone(),
                display,
                row_anchor(5),
                Box::new(|action| match action {
                    Action::Reboot => "> Reboot".into(),
                    _ => "  Reboot".into(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                highlighted.clone(),
                display,
                row_anchor(6),
                Box::new(|action| match action {
                    Action::Help => "> Help".into(),
                    _ => "  Help".into(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                highlighted.clone(),
                display,
                row_anchor(7),
                Box::new(|action| match action {
                    Action::SetupMode => "> Setup Mode".into(),
                    _ => "  Setup Mode".into(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                highlighted.clone(),
                display,
                row_anchor(8),
                Box::new(|action| match action {
                    Action::Updates => "> Updates".into(),
                    _ => "  Updates".into(),
                }),
            )
        });

        let reboot_message = ui.reboot_message.clone();
        let setup_mode = ui.res.setup_mode.setup_mode.clone();
        let show_help = ui.res.setup_mode.show_help.clone();
        let alerts = ui.alerts.clone();

        let active = Active {
            widgets,
            highlighted,
            reboot_message,
            setup_mode,
            show_help,
            alerts,
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
        let action = self.highlighted.try_get().unwrap_or(Action::Reboot);

        // Actions on this page are only allowed with Source::Local
        // (in contrast to Source::Web) to prevent e.g. an attacker from
        // re-enabling the setup mode.

        match ev {
            InputEvent::ToggleAction(Source::Local) => self.highlighted.set(action.next()),
            InputEvent::PerformAction(Source::Local) => match action {
                Action::Reboot => self.reboot_message.set(Some(
                    "Really reboot?\nLong press lower\nbutton to confirm.".to_string(),
                )),
                Action::Help => self.show_help.set(true),
                Action::SetupMode => self.setup_mode.set(true),
                Action::Updates => self.alerts.assert(AlertScreen::UpdateAvailable),
            },
            _ => {}
        }
    }
}
