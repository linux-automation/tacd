use std::time::Duration;

use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task::{block_on, sleep, spawn, spawn_blocking};

use async_trait::async_trait;

use serde::{Deserialize, Serialize};

use tide::{Response, Server};

use evdev::{EventType, InputEventKind, Key};

use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::Text,
};

use crate::broker::{BrokerBuilder, Topic};

mod breakout_screen;
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

use breakout_screen::BreakoutScreen;
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

pub const VERY_LONG_PRESS: Duration = Duration::from_secs(10);
pub const LONG_PRESS: Duration = Duration::from_millis(750);

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum Screen {
    DutPower,
    Usb,
    DigOut,
    System,
    IoBus,
    Uart,
    ScreenSaver,
    Breakout,
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
            Self::Breakout => Self::ScreenSaver,
            Self::RebootConfirm => Self::System,
            Self::Rauc => Self::ScreenSaver,
        }
    }

    /// Should screensaver be automatically enabled when in this screen?
    fn use_screensaver(&self) -> bool {
        match self {
            Self::Rauc => false,
            _ => true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum ButtonEvent {
    ButtonOne(Duration),
    ButtonTwo(Duration),
}

impl ButtonEvent {
    fn from_id(d: Duration, id: usize) -> Self {
        match id {
            0 => Self::ButtonOne(d),
            1 => Self::ButtonTwo(d),
            _ => panic!(),
        }
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

pub struct UiRessources {
    pub adc: crate::adc::Adc,
    pub dbus: crate::dbus::DbusClient,
    pub dig_io: crate::digital_io::DigitalIo,
    pub dut_pwr: crate::dut_power::DutPwrThread,
    pub iobus: crate::iobus::IoBus,
    pub system: crate::system::System,
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
    res: UiRessources,
}

/// Spawn a thread that blockingly reads user input and pushes them into
/// a broker framework topic.
fn handle_button(path: &'static str, topic: Arc<Topic<ButtonEvent>>) {
    spawn_blocking(move || {
        let mut device = evdev::Device::open(path).unwrap();

        let mut start_time = [None, None];

        loop {
            for ev in device.fetch_events().unwrap() {
                if ev.event_type() != EventType::KEY {
                    continue;
                }

                let id = match ev.kind() {
                    InputEventKind::Key(Key::KEY_HOME) => 0,
                    InputEventKind::Key(Key::KEY_ESC) => 1,
                    _ => continue,
                };

                if ev.value() == 0 {
                    // Button release -> send event
                    if let Some(start) = start_time[id].take() {
                        if let Ok(duration) = ev.timestamp().duration_since(start) {
                            block_on(topic.set(ButtonEvent::from_id(duration, id)))
                        }
                    }
                } else {
                    // Button press -> register start time but don't send event
                    start_time[id] = Some(ev.timestamp())
                }
            }
        }
    });
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
    pub fn new(bb: &mut BrokerBuilder, res: UiRessources, server: &mut Server<()>) -> Self {
        let screen = bb.topic_rw("/v1/tac/display/screen", Some(Screen::ScreenSaver));
        let locator = bb.topic_rw("/v1/tac/display/locator", Some(false));
        let locator_dance = bb.topic_ro("/v1/tac/display/locator_dance", None);
        let buttons = bb.topic_rw("/v1/tac/display/buttons", None);

        // Initialize all the screens now so they can be mounted later
        let screens = {
            let mut s: Vec<Box<dyn MountableScreen>> = Vec::new();

            s.push(Box::new(BreakoutScreen::new()));
            s.push(Box::new(DigOutScreen::new(bb)));
            s.push(Box::new(IoBusScreen::new()));
            s.push(Box::new(PowerScreen::new()));
            s.push(Box::new(RaucScreen::new(&screen, &res.dbus.rauc.operation)));
            s.push(Box::new(RebootConfirmScreen::new()));
            s.push(Box::new(ScreenSaverScreen::new(
                bb,
                &buttons,
                &screen,
                &res.dbus.network.hostname,
            )));
            s.push(Box::new(SystemScreen::new()));
            s.push(Box::new(UartScreen::new(bb)));
            s.push(Box::new(UsbScreen::new(bb)));

            s
        };

        handle_button(
            &"/dev/input/by-path/platform-gpio-keys-event",
            buttons.clone(),
        );

        // Blinking locator for the locator widget
        let locator_task = locator.clone();
        let locator_dance_task = locator_dance.clone();
        spawn(async move {
            let (mut rx, _) = locator_task.clone().subscribe_unbounded().await;

            loop {
                while let Some(true) = locator_task.get().await.as_deref().as_deref() {
                    for i in (0..64).rev() {
                        locator_dance_task.set(i).await;
                        sleep(Duration::from_millis(100)).await;
                    }
                }

                match rx.next().await.as_deref().as_deref() {
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
                Some(screen) => next_screen_type = *screen,
                None => break Ok(()),
            }
        }
    }
}
