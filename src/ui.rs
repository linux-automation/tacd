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

use std::time::Duration;

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use futures::{select, FutureExt};
use tide::{Response, Server};

use crate::broker::{BrokerBuilder, Topic};
use crate::led::{BlinkPattern, BlinkPatternBuilder};
use crate::watched_tasks::WatchedTasksBuilder;

mod alerts;
mod buttons;
mod display;
mod screens;
mod widgets;

use alerts::{AlertList, Alerter};
use buttons::{handle_buttons, Button, ButtonEvent, Direction, PressDuration, Source};
pub use display::{Display, ScreenShooter};
pub use screens::message;
use screens::{splash, ActivatableScreen, AlertScreen, NormalScreen, Screen};

pub struct UiResources {
    pub adc: crate::adc::Adc,
    pub backlight: crate::backlight::Backlight,
    pub dig_io: crate::digital_io::DigitalIo,
    pub dut_pwr: crate::dut_power::DutPwrThread,
    pub hostname: crate::dbus::Hostname,
    pub iobus: crate::iobus::IoBus,
    pub led: crate::led::Led,
    pub network: crate::dbus::Network,
    pub rauc: crate::dbus::Rauc,
    pub regulators: crate::regulators::Regulators,
    pub setup_mode: crate::setup_mode::SetupMode,
    pub system: crate::system::System,
    pub systemd: crate::dbus::Systemd,
    pub temperatures: crate::temperatures::Temperatures,
    pub usb_hub: crate::usb_hub::UsbHub,
}

pub struct Ui {
    screen: Arc<Topic<NormalScreen>>,
    alerts: Arc<Topic<AlertList>>,
    locator: Arc<Topic<bool>>,
    buttons: Arc<Topic<ButtonEvent>>,
    screens: Vec<Box<dyn ActivatableScreen>>,
    reboot_message: Arc<Topic<Option<String>>>,
    res: UiResources,
}

enum InputEvent {
    NextScreen,
    ToggleAction(Source),
    PerformAction(Source),
}

impl InputEvent {
    fn from_button(ev: ButtonEvent) -> Option<Self> {
        match ev {
            ButtonEvent {
                dir: Direction::Press,
                btn: Button::Upper,
                dur: PressDuration::Short,
                src: _,
            } => Some(Self::NextScreen),
            ButtonEvent {
                dir: Direction::Release,
                btn: Button::Lower,
                dur: PressDuration::Short,
                src,
            } => Some(Self::ToggleAction(src)),
            ButtonEvent {
                dir: Direction::Press,
                btn: Button::Lower,
                dur: PressDuration::Long,
                src,
            } => Some(Self::PerformAction(src)),
            _ => None,
        }
    }
}

pub fn setup_display() -> Display {
    let display = Display::new();

    display.clear();
    display.with_lock(splash);

    display
}

/// Add a web endpoint that serves the current display content as png
pub fn serve_display(server: &mut Server<()>, screenshooter: ScreenShooter) {
    server.at("/v1/tac/display/content").get(move |_| {
        let png = screenshooter.as_png();

        async move {
            Ok(Response::builder(200)
                .content_type("image/png")
                .header("Cache-Control", "no-store")
                .body(png)
                .build())
        }
    });
}

impl Ui {
    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        res: UiResources,
    ) -> Result<Self> {
        let screen = bb.topic_rw("/v1/tac/display/screen", Some(NormalScreen::first()));
        let locator = bb.topic_rw("/v1/tac/display/locator", Some(false));
        let buttons = bb.topic("/v1/tac/display/buttons", true, true, false, None, 0);
        let alerts = bb.topic_ro("/v1/tac/display/alerts", Some(AlertList::new()));
        let reboot_message = Topic::anonymous(None);

        alerts.assert(AlertScreen::ScreenSaver);

        // Initialize all the screens now so they can be activated later
        let screens = screens::init(wtb, &res, &alerts, &buttons, &reboot_message, &locator)?;

        handle_buttons(
            wtb,
            "/dev/input/by-path/platform-gpio-keys-event",
            buttons.clone(),
        )?;

        // Blink the status LED when locator is active
        let led_status_pattern = res.led.status.clone();
        let led_status_color = res.led.status_color.clone();
        let (mut locator_stream, _) = locator.clone().subscribe_unbounded();
        wtb.spawn_task("locator-led-updater", async move {
            let pattern_locator_on = BlinkPatternBuilder::new(0.0)
                .fade_to(1.0, Duration::from_millis(100))
                .stay_for(Duration::from_millis(300))
                .fade_to(0.0, Duration::from_millis(100))
                .stay_for(Duration::from_millis(500))
                .forever();

            let pattern_locator_off = BlinkPattern::solid(1.0);

            while let Some(ev) = locator_stream.next().await {
                if ev {
                    // White blinking when locator is on
                    led_status_color.set((1.0, 1.0, 1.0));
                    led_status_pattern.set(pattern_locator_on.clone());
                } else {
                    // Green light when locator is off
                    led_status_color.set((0.0, 0.23, 0.0));
                    led_status_pattern.set(pattern_locator_off.clone());
                }
            }

            Ok(())
        })?;

        Ok(Self {
            screen,
            alerts,
            locator,
            buttons,
            screens,
            reboot_message,
            res,
        })
    }

    pub async fn render_loop(mut self, display: Display) -> Result<(), std::io::Error> {
        let (mut screen_rx, _) = self.screen.clone().subscribe_unbounded();
        let (mut alerts_rx, _) = self.alerts.clone().subscribe_unbounded();
        let (mut button_events, _) = self.buttons.clone().subscribe_unbounded();

        // Helper to go to the next screen and activate the screensaver after
        // cycling once.
        let cycle_screen = {
            let screen = self.screen.clone();
            let alerts = self.alerts.clone();

            move || {
                let cur = screen.try_get().unwrap_or_else(NormalScreen::first);
                let next = cur.next();
                screen.set(next);

                if next == NormalScreen::first() {
                    alerts.assert(AlertScreen::ScreenSaver);
                }
            }
        };

        // Take the screens out of self so we can hand out references to self
        // to the screen mounting methods.
        let mut screens = {
            let mut decoy = Vec::new();
            std::mem::swap(&mut self.screens, &mut decoy);
            decoy
        };

        let mut screen = screen_rx.next().await.unwrap();
        let mut alerts = alerts_rx.next().await.unwrap();

        let mut showing = alerts
            .highest_priority()
            .map(Screen::Alert)
            .unwrap_or(Screen::Normal(screen));

        let mut display = Some(display);

        'exit: loop {
            let mut active_screen = {
                let display = display.take().unwrap();
                display.clear();

                screens
                    .iter_mut()
                    .find(|s| s.my_type() == showing)
                    .unwrap()
                    .activate(&self, display)
            };

            'this_screen: loop {
                select! {
                    new = screen_rx.next().fuse() => match new {
                        Some(new) => screen = new,
                        None => break 'exit,
                    },
                    new = alerts_rx.next().fuse() => match new {
                        Some(new) => alerts = new,
                        None => break 'exit,
                    },
                    ev = button_events.next().fuse() => match ev {
                        Some(ev) => {
                            let st = active_screen.my_type();
                            let ev = InputEvent::from_button(ev);

                            // The NextScreen event for normal screens can be handled
                            // here.
                            // The situation for alerts is a bit more complicated.
                            // (Some ignore all input. Some acknoledge via the upper button).
                            // Leave handling for NextScreen to them.

                            match (st, ev) {
                                 (Screen::Normal(_), Some(InputEvent::NextScreen)) => cycle_screen(),
                                 (_, Some(ev)) => active_screen.input(ev),
                                 (_, None) => {}
                            }
                        },
                        None => break 'exit,
                    },

                }

                // Show the highest priority alert (if one is asserted)
                // or a normal screen instead.
                let showing_next = alerts
                    .highest_priority()
                    .map(Screen::Alert)
                    .unwrap_or(Screen::Normal(screen));

                // Tear down this screen if another one should be shown.
                // Otherwise just continue looping.
                if showing_next != showing {
                    showing = showing_next;
                    break 'this_screen;
                }
            }

            display = Some(active_screen.deactivate().await);
        }

        Ok(())
    }

    pub fn run(self, wtb: &mut WatchedTasksBuilder, display: Display) -> Result<()> {
        wtb.spawn_task("screen-render-loop", async move {
            self.render_loop(display).await?;

            Ok(())
        })
    }
}
