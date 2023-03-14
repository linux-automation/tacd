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
use async_std::sync::{Arc, Mutex};
use async_std::task::{sleep, spawn};

use async_trait::async_trait;

use serde::{Deserialize, Serialize};

use tide::{Response, Server};

use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::Text,
};

use crate::broker::{BrokerBuilder, Topic};

mod buttons;
mod dig_out_screen;
mod draw_fb;
mod iobus_screen;
mod power_screen;
mod rauc_screen;
mod reboot_screen;
mod screensaver_screen;
mod system_screen;
mod uart_screen;
mod usb_screen;
mod widgets;

use buttons::{handle_buttons, ButtonEvent};
use dig_out_screen::DigOutScreen;
use draw_fb::FramebufferDrawTarget;
use iobus_screen::IoBusScreen;
use power_screen::PowerScreen;
use rauc_screen::RaucScreen;
use reboot_screen::RebootConfirmScreen;
use screensaver_screen::ScreenSaverScreen;
use system_screen::SystemScreen;
use uart_screen::UartScreen;
use usb_screen::UsbScreen;

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum Screen {
    DutPower,
    Usb,
    DigOut,
    System,
    IoBus,
    Uart,
    ScreenSaver,
    RebootConfirm,
    Rauc,
}

impl Screen {
    /// What is the next screen to transition to when e.g. the button is  pressed?
    fn next(&self) -> Self {
        match self {
            Self::DutPower => Self::Usb,
            Self::Usb => Self::DigOut,
            Self::DigOut => Self::System,
            Self::System => Self::IoBus,
            Self::IoBus => Self::Uart,
            Self::Uart => Self::ScreenSaver,
            Self::ScreenSaver => Self::DutPower,
            Self::RebootConfirm => Self::System,
            Self::Rauc => Self::ScreenSaver,
        }
    }

    /// Should screensaver be automatically enabled when in this screen?
    fn use_screensaver(&self) -> bool {
        !matches!(self, Self::Rauc)
    }
}

#[async_trait]
trait MountableScreen: Sync + Send {
    fn is_my_type(&self, screen: Screen) -> bool;
    async fn mount(&mut self, ui: &Ui);
    async fn unmount(&mut self);
}

/// Draw static screen border contining a title and an indicator for the
/// position of the screen in the list of screens.
async fn draw_border(text: &str, screen: Screen, draw_target: &Arc<Mutex<FramebufferDrawTarget>>) {
    let mut draw_target = draw_target.lock().await;

    Text::new(
        text,
        Point::new(4, 13),
        MonoTextStyle::new(&FONT_8X13, BinaryColor::On),
    )
    .draw(&mut *draw_target)
    .unwrap();

    Line::new(Point::new(0, 16), Point::new(118, 16))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
        .draw(&mut *draw_target)
        .unwrap();

    let screen_idx = screen as i32;
    let num_screens = Screen::ScreenSaver as i32;
    let x_start = screen_idx * 128 / num_screens;
    let x_end = (screen_idx + 1) * 128 / num_screens;

    Line::new(Point::new(x_start, 62), Point::new(x_end, 62))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
        .draw(&mut *draw_target)
        .unwrap();
}

pub struct UiResources {
    pub adc: crate::adc::Adc,
    pub dig_io: crate::digital_io::DigitalIo,
    pub dut_pwr: crate::dut_power::DutPwrThread,
    pub iobus: crate::iobus::IoBus,
    pub network: crate::dbus::Network,
    pub rauc: crate::dbus::Rauc,
    pub system: crate::system::System,
    pub systemd: crate::dbus::Systemd,
    pub temperatures: crate::temperatures::Temperatures,
    pub usb_hub: crate::usb_hub::UsbHub,
}

pub struct Ui {
    draw_target: Arc<Mutex<FramebufferDrawTarget>>,
    screen: Arc<Topic<Screen>>,
    locator: Arc<Topic<bool>>,
    locator_dance: Arc<Topic<i32>>,
    buttons: Arc<Topic<ButtonEvent>>,
    screens: Vec<Box<dyn MountableScreen>>,
    res: UiResources,
}

/// Add a web endpoint that serves the current framebuffer as png
fn serve_framebuffer(server: &mut Server<()>, draw_target: Arc<Mutex<FramebufferDrawTarget>>) {
    server.at("/v1/tac/display/content").get(move |_| {
        let draw_target = draw_target.clone();

        async move {
            Ok(Response::builder(200)
                .content_type("image/png")
                .header("Cache-Control", "no-store")
                .body(draw_target.lock().await.as_png())
                .build())
        }
    });
}

impl Ui {
    pub fn new(bb: &mut BrokerBuilder, res: UiResources, server: &mut Server<()>) -> Self {
        let screen = bb.topic_rw("/v1/tac/display/screen", Some(Screen::ScreenSaver));
        let locator = bb.topic_rw("/v1/tac/display/locator", Some(false));
        let locator_dance = bb.topic_ro("/v1/tac/display/locator_dance", Some(0));
        let buttons = bb.topic("/v1/tac/display/buttons", true, true, None, 0);

        // Initialize all the screens now so they can be mounted later
        let screens: Vec<Box<dyn MountableScreen>> = vec![
            Box::new(DigOutScreen::new(bb)),
            Box::new(IoBusScreen::new()),
            Box::new(PowerScreen::new()),
            Box::new(RaucScreen::new(&screen, &res.rauc.operation)),
            Box::new(RebootConfirmScreen::new()),
            Box::new(ScreenSaverScreen::new(&buttons, &screen)),
            Box::new(SystemScreen::new()),
            Box::new(UartScreen::new(bb)),
            Box::new(UsbScreen::new(bb)),
        ];

        handle_buttons(
            "/dev/input/by-path/platform-gpio-keys-event",
            buttons.clone(),
        );

        // Animated Locator for the locator widget
        let locator_task = locator.clone();
        let locator_dance_task = locator_dance.clone();
        spawn(async move {
            let (mut rx, _) = locator_task.clone().subscribe_unbounded().await;

            loop {
                // As long as the locator is active:
                // count down the value in locator_dance from 63 to 0
                // with some pause in between in a loop.
                while locator_task.try_get().await.unwrap_or(false) {
                    locator_dance_task
                        .modify(|v| match v {
                            None | Some(0) => Some(63),
                            Some(v) => Some(v - 1),
                        })
                        .await;
                    sleep(Duration::from_millis(100)).await;
                }

                // If the locator is empty stop the animation
                locator_dance_task.set(0).await;

                match rx.next().await {
                    Some(true) => {}
                    Some(false) => continue,
                    None => break,
                }
            }
        });

        let draw_target = Arc::new(Mutex::new(FramebufferDrawTarget::new()));

        // Expose the framebuffer as png via the web interface
        serve_framebuffer(server, draw_target.clone());

        Self {
            draw_target,
            screen,
            locator,
            locator_dance,
            buttons,
            screens,
            res,
        }
    }

    pub async fn run(mut self) -> Result<(), std::io::Error> {
        let (mut screen_rx, _) = self.screen.clone().subscribe_unbounded().await;

        // Take the screens out of self so we can hand out references to self
        // to the screen mounting methods.
        let mut screens = {
            let mut decoy = Vec::new();
            std::mem::swap(&mut self.screens, &mut decoy);
            decoy
        };

        let mut curr_screen_type = None;
        let mut next_screen_type = Screen::ScreenSaver;

        loop {
            // Only unmount / mount the shown screen if a change was requested
            let should_change = curr_screen_type
                .map(|c| c != next_screen_type)
                .unwrap_or(true);

            if should_change {
                // Find the currently shown screen (if any) and unmount it
                if let Some(curr) = curr_screen_type {
                    if let Some(screen) = screens.iter_mut().find(|s| s.is_my_type(curr)) {
                        screen.unmount().await;
                    }
                }

                // Clear the screen as static elements are not cleared by the
                // widget framework magic
                self.draw_target.lock().await.clear();

                // Find the screen to show (if any) and "mount" it
                // (e.g. tell it to handle the screen by itself).
                if let Some(screen) = screens.iter_mut().find(|s| s.is_my_type(next_screen_type)) {
                    screen.mount(&self).await;
                }

                curr_screen_type = Some(next_screen_type);
            }

            match screen_rx.next().await {
                Some(screen) => next_screen_type = screen,
                None => break Ok(()),
            }
        }
    }
}
