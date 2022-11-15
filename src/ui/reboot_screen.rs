use async_std::prelude::*;
use async_std::task::spawn;
use async_trait::async_trait;

use crate::broker::{Native, SubscriptionHandle};
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Text},
};

use super::{widgets::*, FramebufferDrawTarget, LONG_PRESS};
use super::{ButtonEvent, MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::RebootConfirm;

pub struct RebootConfirmScreen {
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl RebootConfirmScreen {
    pub fn new() -> Self {
        Self {
            buttons_handle: None,
        }
    }
}

fn rly(draw_target: &mut FramebufferDrawTarget) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    Text::with_alignment(
        "Really reboot?\nLong press to confirm",
        Point::new(64, 28),
        text_style,
        Alignment::Center,
    )
    .draw(draw_target)
    .unwrap();
}

fn brb(draw_target: &mut FramebufferDrawTarget) {
    let text_style: MonoTextStyle<BinaryColor> = MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

    Text::with_alignment(
        "Hold tight\nBe right back",
        Point::new(64, 28),
        text_style,
        Alignment::Center,
    )
    .draw(draw_target)
    .unwrap();
}

#[async_trait]
impl MountableScreen for RebootConfirmScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        let draw_target = ui.draw_target.clone();
        rly(&mut *draw_target.lock().await);

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let screen = ui.screen.clone();
        let reboot = ui.res.system.reboot.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                if let ButtonEvent::ButtonOne(dur) = *ev {
                    if dur > LONG_PRESS {
                        brb(&mut *draw_target.lock().await);
                        reboot.set(true).await;
                        break;
                    }
                }

                screen.set(SCREEN_TYPE.next()).await;
            }
        });

        self.buttons_handle = Some(buttons_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.buttons_handle.take() {
            handle.unsubscribe().await;
        }
    }
}
