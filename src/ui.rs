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

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{sleep, spawn};
use futures::{select, FutureExt};
use tide::{Response, Server};

use crate::broker::{BrokerBuilder, Topic};
use crate::led::{BlinkPattern, BlinkPatternBuilder};

mod buttons;
mod display;
mod screens;
mod widgets;

use buttons::{handle_buttons, Button, ButtonEvent, PressDuration, Source};
use display::{Display, ScreenShooter};
use screens::{splash, ActivatableScreen, Screen};

pub struct UiResources {
    pub adc: crate::adc::Adc,
    pub dig_io: crate::digital_io::DigitalIo,
    pub dut_pwr: crate::dut_power::DutPwrThread,
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
    screen: Arc<Topic<Screen>>,
    locator: Arc<Topic<bool>>,
    locator_dance: Arc<Topic<i32>>,
    buttons: Arc<Topic<ButtonEvent>>,
    screens: Vec<Box<dyn ActivatableScreen>>,
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
            ButtonEvent::Release {
                btn: Button::Upper,
                dur: _,
                src: _,
            } => Some(Self::NextScreen),
            ButtonEvent::Release {
                btn: Button::Lower,
                dur: PressDuration::Short,
                src,
            } => Some(Self::ToggleAction(src)),
            ButtonEvent::Release {
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
    pub fn new(bb: &mut BrokerBuilder, res: UiResources) -> Self {
        let screen = bb.topic_rw("/v1/tac/display/screen", Some(Screen::ScreenSaver));
        let locator = bb.topic_rw("/v1/tac/display/locator", Some(false));
        let locator_dance = bb.topic_ro("/v1/tac/display/locator_dance", Some(0));
        let buttons = bb.topic("/v1/tac/display/buttons", true, true, false, None, 0);

        // Initialize all the screens now so they can be mounted later
        let screens: Vec<Box<_>> = screens::init(&res, &screen, &buttons);

        handle_buttons(
            "/dev/input/by-path/platform-gpio-keys-event",
            buttons.clone(),
        );

        // Animated Locator for the locator widget
        let locator_task = locator.clone();
        let locator_dance_task = locator_dance.clone();
        spawn(async move {
            let (mut rx, _) = locator_task.clone().subscribe_unbounded();

            loop {
                // As long as the locator is active:
                // count down the value in locator_dance from 63 to 0
                // with some pause in between in a loop.
                while locator_task.try_get().unwrap_or(false) {
                    locator_dance_task.modify(|v| match v {
                        None | Some(0) => Some(63),
                        Some(v) => Some(v - 1),
                    });
                    sleep(Duration::from_millis(100)).await;
                }

                // If the locator is empty stop the animation
                locator_dance_task.set(0);

                match rx.next().await {
                    Some(true) => {}
                    Some(false) => continue,
                    None => break,
                }
            }
        });

        // Blink the status LED when locator is active
        let led_status_pattern = res.led.status.clone();
        let led_status_color = res.led.status_color.clone();
        let (mut locator_stream, _) = locator.clone().subscribe_unbounded();
        spawn(async move {
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
                    led_status_color.set((0.0, 1.0, 0.0));
                    led_status_pattern.set(pattern_locator_off.clone());
                }
            }
        });

        Self {
            screen,
            locator,
            locator_dance,
            buttons,
            screens,
            res,
        }
    }

    pub async fn run(mut self, display: Display) -> Result<(), std::io::Error> {
        let (mut screen_rx, _) = self.screen.clone().subscribe_unbounded();
        let (mut button_events, _) = self.buttons.clone().subscribe_unbounded();

        // Take the screens out of self so we can hand out references to self
        // to the screen mounting methods.
        let mut screens = {
            let mut decoy = Vec::new();
            std::mem::swap(&mut self.screens, &mut decoy);
            decoy
        };

        let mut showing = screen_rx.next().await.unwrap();
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
                        Some(new) => {
                            showing = new;
                            break 'this_screen;
                        }
                        None => break 'exit,
                    },
                    ev = button_events.next().fuse() => match ev {
                        Some(ev) => {
                            let ev = InputEvent::from_button(ev);

                            if let Some(ev) = ev {
                                active_screen.input(ev);
                            }
                        },
                        None => break 'exit,
                    },

                }
            }

            display = Some(active_screen.deactivate().await);
        }

        Ok(())
    }
}
