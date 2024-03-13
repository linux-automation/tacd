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

use anyhow::Result;
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
mod iobus_health;
mod locator;
mod overtemperature;
mod power;
mod power_fail;
mod reboot;
mod screensaver;
mod setup;
mod system;
mod uart;
mod update_available;
mod update_installation;
mod usb;
mod usb_overload;

use dig_out::DigOutScreen;
use help::HelpScreen;
use iobus::IoBusScreen;
use iobus_health::IoBusHealthScreen;
use locator::LocatorScreen;
use overtemperature::OverTemperatureScreen;
use power::PowerScreen;
use power_fail::PowerFailScreen;
use reboot::RebootConfirmScreen;
use screensaver::ScreenSaverScreen;
use setup::SetupScreen;
use system::SystemScreen;
use uart::UartScreen;
use update_available::UpdateAvailableScreen;
use update_installation::UpdateInstallationScreen;
use usb::UsbScreen;
use usb_overload::UsbOverloadScreen;

use super::buttons;
use super::widgets;
use super::{AlertList, Alerter, InputEvent, Ui, UiResources};
use crate::ui::display::{Display, DisplayExclusive};
use crate::{broker::Topic, watched_tasks::WatchedTasksBuilder};
use buttons::ButtonEvent;
use widgets::UI_TEXT_FONT;

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Clone, Copy, Debug)]
pub enum NormalScreen {
    DutPower,
    Usb,
    DigOut,
    System,
    IoBus,
    Uart,
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Clone, Copy, Debug)]
pub enum AlertScreen {
    ScreenSaver,
    IoBusHealth,
    PowerFail,
    Locator,
    RebootConfirm,
    UpdateAvailable,
    UpdateInstallation,
    UsbOverload,
    Help,
    Setup,
    OverTemperature,
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Clone, Copy, Debug)]
pub enum Screen {
    Normal(NormalScreen),
    Alert(AlertScreen),
}

impl NormalScreen {
    pub fn first() -> Self {
        Self::DutPower
    }

    /// What is the next screen to transition to when e.g. the button is  pressed?
    pub fn next(&self) -> Self {
        match self {
            Self::DutPower => Self::Usb,
            Self::Usb => Self::DigOut,
            Self::DigOut => Self::System,
            Self::System => Self::IoBus,
            Self::IoBus => Self::Uart,
            Self::Uart => Self::DutPower,
        }
    }
}

#[async_trait]
pub(super) trait ActiveScreen: Send {
    fn my_type(&self) -> Screen;
    async fn deactivate(self: Box<Self>) -> Display;
    fn input(&mut self, ev: InputEvent);
}

pub(super) trait ActivatableScreen: Sync + Send {
    fn my_type(&self) -> Screen;
    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen>;
}

/// Draw static screen border containing a title and an indicator for the
/// position of the screen in the list of screens.
fn draw_border(target: &mut DisplayExclusive, text: &str, screen: NormalScreen) {
    Text::new(
        text,
        Point::new(8, 17),
        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On),
    )
    .draw(target)
    .unwrap();

    Line::new(Point::new(0, 23), Point::new(240, 23))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
        .draw(target)
        .unwrap();

    let screen_idx = screen as i32;
    let num_screens = (NormalScreen::Uart as i32) + 1;
    let x_start = screen_idx * 240 / num_screens;
    let x_end = (screen_idx + 1) * 240 / num_screens;

    Line::new(Point::new(x_start, 240), Point::new(x_end, 240))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 4))
        .draw(target)
        .unwrap();
}

const fn row_anchor(row_num: u8) -> Point {
    assert!(row_num < 9);

    Point::new(8, 52 + (row_num as i32) * 20)
}

pub fn message(target: &mut DisplayExclusive, text: &str) -> Rectangle {
    let ui_text_style: MonoTextStyle<BinaryColor> =
        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    let mut text = Text::with_alignment(text, Point::zero(), ui_text_style, Alignment::Center);

    let offset = Point::new(120, 120) - text.bounding_box().center();
    text.translate_mut(offset);

    text.draw(target).unwrap();

    text.bounding_box()
}

pub fn splash(target: &mut DisplayExclusive) -> Rectangle {
    message(target, "Welcome")
}

pub(super) fn init(
    wtb: &mut WatchedTasksBuilder,
    res: &UiResources,
    alerts: &Arc<Topic<AlertList>>,
    buttons: &Arc<Topic<ButtonEvent>>,
    reboot_message: &Arc<Topic<Option<String>>>,
    locator: &Arc<Topic<bool>>,
) -> Result<Vec<Box<dyn ActivatableScreen>>> {
    Ok(vec![
        Box::new(DigOutScreen::new()),
        Box::new(IoBusScreen::new()),
        Box::new(PowerScreen::new()),
        Box::new(SystemScreen::new()),
        Box::new(UartScreen::new()),
        Box::new(UsbScreen::new()),
        Box::new(HelpScreen::new(wtb, alerts, &res.setup_mode.show_help)?),
        Box::new(IoBusHealthScreen::new(
            wtb,
            alerts,
            &res.iobus.supply_fault,
        )?),
        Box::new(UpdateInstallationScreen::new(
            wtb,
            alerts,
            &res.rauc.operation,
            reboot_message,
            &res.rauc.should_reboot,
        )?),
        Box::new(UpdateAvailableScreen::new(wtb, alerts, &res.rauc.channels)?),
        Box::new(RebootConfirmScreen::new(wtb, alerts, reboot_message)?),
        Box::new(ScreenSaverScreen::new(wtb, buttons, alerts)?),
        Box::new(SetupScreen::new(wtb, alerts, &res.setup_mode.setup_mode)?),
        Box::new(OverTemperatureScreen::new(
            wtb,
            alerts,
            &res.temperatures.warning,
        )?),
        Box::new(LocatorScreen::new(wtb, alerts, locator)?),
        Box::new(UsbOverloadScreen::new(wtb, alerts, &res.usb_hub.overload)?),
        Box::new(PowerFailScreen::new(wtb, alerts, &res.dut_pwr.state)?),
    ])
}
