// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use async_trait::async_trait;
use embedded_graphics::{prelude::Point, text::Alignment};
use serde::{Deserialize, Serialize};

use super::buttons::Source;
use super::widgets::*;
use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display, InputEvent, Screen,
    Ui,
};
use crate::broker::{Native, SubscriptionHandle, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::Setup;

#[derive(Serialize, Deserialize, Clone)]
enum Connectivity {
    Nothing,
    HostnameOnly(String),
    IpOnly(String),
    Both(String, String),
}

pub struct SetupScreen;

struct Active {
    widgets: WidgetContainer,
    hostname_update_handle: SubscriptionHandle<String, Native>,
    ip_update_handle: SubscriptionHandle<Vec<String>, Native>,
    alerts: Arc<Topic<AlertList>>,
    diagnostics_presses: u8,
}

impl SetupScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        setup_mode: &Arc<Topic<bool>>,
    ) -> Result<Self> {
        let (mut setup_mode_events, _) = setup_mode.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-setup-avtivator", async move {
            while let Some(setup_mode) = setup_mode_events.next().await {
                if setup_mode {
                    alerts.assert(AlertScreen::Setup);
                } else {
                    alerts.deassert(AlertScreen::Setup);
                }
            }

            Ok(())
        })?;

        Ok(Self)
    }
}

impl ActivatableScreen for SetupScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        /* We want to display hints on how to connect to this TAC.
         * We want to show:
         * - An URL based on the hostname, e.g. http://lxatac-12345
         * - An URL based on an IP[1], e.g. http://192.168.1.1
         * - Both
         *
         * This information may not be immediately available on boot,
         * so we collect it in connectivity_topic and update it once it comes
         * in.
         *
         * [1]: We can barely fit a maximum-length IPv4 address in one line,
         * so we currently opt out of showing an IPv6 based URL as well.
         * It would most likely be too long to practically read it and type into a
         * browser anyways. */
        let connectivity_topic = Topic::anonymous(Some(Connectivity::Nothing));

        let connectivity_topic_task = connectivity_topic.clone();
        let (mut hostname_stream, hostname_update_handle) =
            ui.res.hostname.hostname.clone().subscribe_unbounded();

        spawn(async move {
            while let Some(hostname) = hostname_stream.next().await {
                connectivity_topic_task.modify(|prev| match prev.unwrap() {
                    Connectivity::Nothing | Connectivity::HostnameOnly(_) => {
                        Some(Connectivity::HostnameOnly(hostname))
                    }
                    Connectivity::IpOnly(ip) | Connectivity::Both(ip, _) => {
                        Some(Connectivity::Both(ip, hostname))
                    }
                });
            }
        });

        let connectivity_topic_task = connectivity_topic.clone();
        let (mut ip_stream, ip_update_handle) = ui
            .res
            .network
            .bridge_interface
            .clone()
            .subscribe_unbounded();

        spawn(async move {
            while let Some(ips) = ip_stream.next().await {
                connectivity_topic_task.modify(|prev| {
                    let ip = ips.first().cloned();

                    match (prev.unwrap(), ip) {
                        (Connectivity::Nothing, Some(ip)) | (Connectivity::IpOnly(_), Some(ip)) => {
                            Some(Connectivity::IpOnly(ip))
                        }
                        (Connectivity::HostnameOnly(hn), Some(ip))
                        | (Connectivity::Both(_, hn), Some(ip)) => Some(Connectivity::Both(ip, hn)),
                        (Connectivity::IpOnly(_), None) | (Connectivity::Nothing, None) => {
                            Some(Connectivity::Nothing)
                        }
                        (Connectivity::HostnameOnly(hn), None)
                        | (Connectivity::Both(_, hn), None) => Some(Connectivity::HostnameOnly(hn)),
                    }
                });
            }
        });

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display|
            DynamicWidget::text_aligned(
                connectivity_topic,
                display,
                Point::new(120, 55),
                Box::new(|connectivity| match connectivity {
                    Connectivity::Nothing => {
                        "Welcome to your TAC!\n\n\nPlease connect\nto a network\nto continue\nthe setup".into()
                    }
                    Connectivity::HostnameOnly(c) | Connectivity::IpOnly(c) => {
                        format!("Welcome to your TAC!\n\nPlease continue the\nsetup at:\n\n\nhttp://{c}")
                    }
                    Connectivity::Both(ip, hn) => format!(
                        "Welcome to your TAC!\n\nPlease continue the\nsetup at:\n\nhttp://{hn}\nor\nhttp://{ip}"
                    ),
                }),
                Alignment::Center,
        ));

        let alerts = ui.alerts.clone();
        let diagnostics_presses = 0;

        let active = Active {
            widgets,
            hostname_update_handle,
            ip_update_handle,
            alerts,
            diagnostics_presses,
        };

        Box::new(active)
    }
}

#[async_trait]
impl ActiveScreen for Active {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    async fn deactivate(mut self: Box<Self>) -> Display {
        self.hostname_update_handle.unsubscribe();
        self.ip_update_handle.unsubscribe();
        self.widgets.destroy().await
    }

    fn input(&mut self, ev: InputEvent) {
        // To activate the diagnostics screen we expect the upper and lower
        // button to be pressed in the following pattern:
        //
        //   Upper, Lower, Upper, Lower, Upper, Lower
        //
        // Long presses, presses of the wrong button or presses via the web
        // API are invalid and reset the counter.
        let expected_button = self.diagnostics_presses % 2;

        let button = match ev {
            InputEvent::NextScreen => 0,
            InputEvent::ToggleAction(Source::Local) => 1,
            _ => 0xff,
        };

        self.diagnostics_presses += 1;

        if expected_button != button {
            self.diagnostics_presses = 0;
        }

        if self.diagnostics_presses >= 6 {
            self.diagnostics_presses = 0;
            self.alerts.assert(AlertScreen::Diagnostics);
        }
    }
}
