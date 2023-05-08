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

use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use serde::{Deserialize, Serialize};

mod dig_out;
mod help;
mod iobus;
mod power;
mod reboot;
mod screensaver;
mod setup;
mod system;
mod uart;
mod update_installation;
mod usb;

use dig_out::DigOutScreen;
use help::HelpScreen;
use iobus::IoBusScreen;
use power::PowerScreen;
use reboot::RebootConfirmScreen;
use screensaver::ScreenSaverScreen;
use setup::SetupScreen;
use system::SystemScreen;
use uart::UartScreen;
use update_installation::UpdateInstallationScreen;
use usb::UsbScreen;

use super::buttons;
use super::widgets;
use super::{Ui, UiResources};
use crate::broker::Topic;
use crate::ui::display::{Display, DisplayExclusive};
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
    UpdateInstallation,
    Setup,
    Help,
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
            Self::UpdateInstallation => Self::ScreenSaver,
            Self::Setup => Self::ScreenSaver,
            Self::Help => Self::ScreenSaver,
        }
    }

    /// Should screensaver be automatically enabled when in this screen?
    fn use_screensaver(&self) -> bool {
        !matches!(self, Self::UpdateInstallation | Self::Setup | Self::Help)
    }
}

#[async_trait]
pub(super) trait ActiveScreen {
    async fn deactivate(self: Box<Self>) -> Display;
}

pub(super) trait ActivatableScreen: Sync + Send {
    fn my_type(&self) -> Screen;
    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen>;
}

/// Draw static screen border containing a title and an indicator for the
/// position of the screen in the list of screens.
fn draw_border(text: &str, screen: Screen, display: &Display) {
    display.with_lock(|target| {
        Text::new(
            text,
            Point::new(8, 17),
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On),
        )
        .draw(target)
        .unwrap();

        Line::new(Point::new(0, 24), Point::new(230, 24))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
            .draw(target)
            .unwrap();

        let screen_idx = screen as i32;
        let num_screens = Screen::ScreenSaver as i32;
        let x_start = screen_idx * 240 / num_screens;
        let x_end = (screen_idx + 1) * 240 / num_screens;

        Line::new(Point::new(x_start, 238), Point::new(x_end, 238))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 4))
            .draw(target)
            .unwrap();
    });
}

const fn row_anchor(row_num: u8) -> Point {
    assert!(row_num < 8);

    Point::new(8, 52 + (row_num as i32) * 20)
}

pub(super) fn splash(target: &mut DisplayExclusive) -> Rectangle {
    let ui_text_style: MonoTextStyle<BinaryColor> =
        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    let text = Text::with_alignment(
        "Welcome",
        Point::new(120, 120),
        ui_text_style,
        Alignment::Center,
    );

    text.draw(target).unwrap();

    text.bounding_box()
}

pub(super) fn init(
    res: &UiResources,
    screen: &Arc<Topic<Screen>>,
    buttons: &Arc<Topic<ButtonEvent>>,
) -> Vec<Box<dyn ActivatableScreen>> {
    vec![
        Box::new(DigOutScreen::new()),
        Box::new(HelpScreen::new()),
        Box::new(IoBusScreen::new()),
        Box::new(PowerScreen::new()),
        Box::new(RebootConfirmScreen::new()),
        Box::new(ScreenSaverScreen::new(buttons, screen)),
        Box::new(SetupScreen::new(screen, &res.setup_mode.setup_mode)),
        Box::new(SystemScreen::new()),
        Box::new(UartScreen::new()),
        Box::new(UpdateInstallationScreen::new(screen, &res.rauc.operation)),
        Box::new(UsbScreen::new()),
    ]
}
