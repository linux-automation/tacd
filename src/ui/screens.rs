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

use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::Text,
};
use serde::{Deserialize, Serialize};

mod dig_out;
mod iobus;
mod power;
mod rauc;
mod reboot;
mod screensaver;
mod system;
mod uart;
mod usb;

use dig_out::DigOutScreen;
use iobus::IoBusScreen;
use power::PowerScreen;
use rauc::RaucScreen;
use reboot::RebootConfirmScreen;
use screensaver::ScreenSaverScreen;
use system::SystemScreen;
use uart::UartScreen;
use usb::UsbScreen;

use super::buttons;
use super::widgets;
use super::{FramebufferDrawTarget, Ui, UiResources};
use crate::broker::Topic;
use buttons::ButtonEvent;
use widgets::UI_TEXT_FONT;

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
pub(super) trait MountableScreen: Sync + Send {
    fn is_my_type(&self, screen: Screen) -> bool;
    async fn mount(&mut self, ui: &Ui);
    async fn unmount(&mut self);
}

/// Draw static screen border containing a title and an indicator for the
/// position of the screen in the list of screens.
async fn draw_border(text: &str, screen: Screen, draw_target: &Arc<Mutex<FramebufferDrawTarget>>) {
    let mut draw_target = draw_target.lock().await;

    Text::new(
        text,
        Point::new(8, 17),
        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On),
    )
    .draw(&mut *draw_target)
    .unwrap();

    Line::new(Point::new(0, 24), Point::new(230, 24))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
        .draw(&mut *draw_target)
        .unwrap();

    let screen_idx = screen as i32;
    let num_screens = Screen::ScreenSaver as i32;
    let x_start = screen_idx * 240 / num_screens;
    let x_end = (screen_idx + 1) * 240 / num_screens;

    Line::new(Point::new(x_start, 238), Point::new(x_end, 238))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 4))
        .draw(&mut *draw_target)
        .unwrap();
}

const fn row_anchor(row_num: u8) -> Point {
    assert!(row_num < 8);

    Point::new(8, 52 + (row_num as i32) * 20)
}

pub(super) fn init(
    res: &UiResources,
    screen: &Arc<Topic<Screen>>,
    buttons: &Arc<Topic<ButtonEvent>>,
) -> Vec<Box<dyn MountableScreen>> {
    vec![
        Box::new(DigOutScreen::new()),
        Box::new(IoBusScreen::new()),
        Box::new(PowerScreen::new()),
        Box::new(RaucScreen::new(screen, &res.rauc.operation)),
        Box::new(RebootConfirmScreen::new()),
        Box::new(ScreenSaverScreen::new(buttons, screen)),
        Box::new(SystemScreen::new()),
        Box::new(UartScreen::new()),
        Box::new(UsbScreen::new()),
    ]
}
