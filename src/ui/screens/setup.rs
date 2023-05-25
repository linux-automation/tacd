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
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use async_trait::async_trait;

use embedded_graphics::{prelude::Point, text::Alignment};
use serde::{Deserialize, Serialize};

use super::widgets::*;
use super::{MountableScreen, Screen, Ui};
use crate::broker::{Native, SubscriptionHandle, Topic};

const SCREEN_TYPE: Screen = Screen::Setup;

#[derive(Serialize, Deserialize, Clone)]
enum Connectivity {
    Nothing,
    HostnameOnly(String),
    IpOnly(String),
    Both(String, String),
}

pub struct SetupScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    hostname_update_handle: Option<SubscriptionHandle<String, Native>>,
    ip_update_handle: Option<SubscriptionHandle<Vec<String>, Native>>,
}

impl SetupScreen {
    pub fn new(screen: &Arc<Topic<Screen>>, setup_mode: &Arc<Topic<bool>>) -> Self {
        let (mut setup_mode_events, _) = setup_mode.clone().subscribe_unbounded();
        let screen_task = screen.clone();
        spawn(async move {
            while let Some(setup_mode) = setup_mode_events.next().await {
                /* If the setup mode is enabled and we are on the setup mode screen
                 * => Do nothing.
                 * If the setup mode is enabled and we are not on the setup mode screen
                 * => Go to the setup mode screen.
                 * If the setup mode is not enabled but we are on its screen
                 * => Go the "next" screen, as specified in screens.rs.
                 * None of the above
                 * => Do nothing. */
                screen_task.modify(|screen| match (setup_mode, screen) {
                    (true, Some(Screen::Setup)) => None,
                    (true, _) => Some(Screen::Setup),
                    (false, Some(Screen::Setup)) => Some(Screen::Setup.next()),
                    (false, _) => None,
                });
            }
        });

        Self {
            widgets: Vec::new(),
            hostname_update_handle: None,
            ip_update_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for SetupScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
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
            ui.res.network.hostname.clone().subscribe_unbounded();

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
                    let ip = ips.get(0).cloned();

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

        self.widgets.push(Box::new(
            DynamicWidget::text_aligned(
                connectivity_topic,
                ui.draw_target.clone(),
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
            )
            ,
        ));

        self.hostname_update_handle = Some(hostname_update_handle);
        self.ip_update_handle = Some(ip_update_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.hostname_update_handle.take() {
            handle.unsubscribe();
        }

        if let Some(handle) = self.ip_update_handle.take() {
            handle.unsubscribe();
        }

        for mut widget in self.widgets.drain(..) {
            widget.unmount().await
        }
    }
}
