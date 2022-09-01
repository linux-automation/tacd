use std::convert::TryInto;
use std::time::Duration;

use async_std::future::timeout;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{sleep, spawn};

use async_trait::async_trait;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X9, MonoFont, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use super::widgets::{AnyWidget, DynamicWidget};
use super::{ButtonEvent, MountableScreen, Screen, Ui, VERY_LONG_PRESS};

use crate::broker::{BrokerBuilder, Native, SubscriptionHandle, Topic};

const UI_TEXT_FONT: MonoFont = FONT_6X9;
const SCREEN_TYPE: Screen = Screen::ScreenSaver;
const SCREENSAVER_TIMEOUT: Duration = Duration::from_secs(60);

/// get the value of a sawtooth wave with max amplitude range at position i
fn bounce(i: u32, range: i32) -> i32 {
    if range > 0 {
        let period = (i % (2 * (range as u32))).try_into().unwrap_or(range);
        (period - range).abs()
    } else {
        0
    }
}

pub struct ScreenSaverScreen {
    hostname_dance: Arc<Topic<(u32, Arc<String>)>>,
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl ScreenSaverScreen {
    pub fn new(
        bb: &mut BrokerBuilder,
        buttons: &Arc<Topic<ButtonEvent>>,
        screen: &Arc<Topic<Screen>>,
        hostname: &Arc<Topic<String>>,
    ) -> Self {
        let hostname_dance = bb.topic_hidden(None);

        // Activate screensaver if no button is pressed for some time
        let buttons_task = buttons.clone();
        let screen_task = screen.clone();
        spawn(async move {
            let (mut buttons_events, _) = buttons_task.subscribe_unbounded().await;

            loop {
                let ev = timeout(SCREENSAVER_TIMEOUT, buttons_events.next()).await;
                let activate_screensaver = match ev {
                    Ok(None) => break,
                    Ok(Some(_)) => false,
                    Err(_) => true,
                };

                if activate_screensaver {
                    screen_task
                        .modify(|screen| {
                            screen.and_then(|s| {
                                if s.use_screensaver() {
                                    Some(Arc::new(Screen::ScreenSaver))
                                } else {
                                    None
                                }
                            })
                        })
                        .await;
                }
            }
        });

        // TODO: could be moved to mount()
        // Animated hostname for the screensaver
        let hostname_task = hostname.clone();
        let hostname_dance_task = hostname_dance.clone();
        spawn(async move {
            let mut i = 0u32;

            loop {
                if let Some(hostname) = hostname_task.get().await {
                    i = i.wrapping_add(1);
                    hostname_dance_task.set((i, (*hostname).clone())).await;
                }

                sleep(Duration::from_millis(100)).await;
            }
        });

        Self {
            hostname_dance,
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for ScreenSaverScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::new(
                self.hostname_dance.clone(),
                ui.draw_target.clone(),
                Point::new(0, 0),
                Box::new(move |msg, _, target| {
                    let (i, hostname) = msg;

                    let ui_text_style: MonoTextStyle<BinaryColor> =
                        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                    let text = Text::new(&hostname, Point::new(0, 0), ui_text_style);

                    let text_dim = text
                        .bounding_box()
                        .bottom_right()
                        .unwrap_or(Point::new(0, 0));

                    let text = text.translate(Point::new(
                        bounce(*i, 118 - text_dim.x),
                        bounce(*i, 64 - text_dim.y) + text_dim.y,
                    ));

                    text.draw(target).unwrap();

                    Some(text.bounding_box())
                }),
            )
            .await,
        ));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let locator = ui.locator.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                if let ButtonEvent::ButtonOne(dur) = *ev {
                    if dur > VERY_LONG_PRESS {
                        screen.set(Screen::Breakout).await
                    } else {
                        let prev = locator.get().await.as_deref().copied().unwrap_or(false);
                        locator.set(!prev).await
                    }
                }

                if let ButtonEvent::ButtonTwo(_) = *ev {
                    screen.set(SCREEN_TYPE.next()).await
                }
            }
        });

        self.buttons_handle = Some(buttons_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.buttons_handle.take() {
            handle.unsubscribe().await;
        }

        for mut widget in self.widgets.drain(..) {
            widget.unmount_any().await
        }
    }
}
