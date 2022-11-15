use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;
use async_trait::async_trait;

use embedded_graphics::prelude::*;

use crate::broker::{BrokerBuilder, Native, SubscriptionHandle, Topic};

use super::widgets::*;
use super::{draw_border, ButtonEvent, MountableScreen, Screen, Ui, LONG_PRESS};

const SCREEN_TYPE: Screen = Screen::Uart;

pub struct UartScreen {
    highlighted: Arc<Topic<u8>>,
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl UartScreen {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        Self {
            highlighted: bb.topic_hidden(),
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for UartScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("DUT UART", SCREEN_TYPE, &ui.draw_target).await;

        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        let ports = [
            (0, "UART RX EN", 29, &ui.res.dig_io.uart_rx_en),
            (1, "UART TX EN", 44, &ui.res.dig_io.uart_tx_en),
        ];

        for (idx, name, y, status) in ports {
            self.widgets.push(Box::new(
                DynamicWidget::text(
                    self.highlighted.clone(),
                    ui.draw_target.clone(),
                    Point::new(0, y),
                    Box::new(move |highlight: &u8| {
                        format!(
                            "{} {}",
                            if *highlight as usize == idx { ">" } else { " " },
                            name,
                        )
                    }),
                )
                .await,
            ));

            self.widgets.push(Box::new(
                DynamicWidget::indicator(
                    status.clone(),
                    ui.draw_target.clone(),
                    Point::new(80, y - 7),
                    Box::new(|state: &bool| match *state {
                        true => IndicatorState::On,
                        false => IndicatorState::Off,
                    }),
                )
                .await,
            ));
        }

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let dir_enables = [
            ui.res.dig_io.uart_rx_en.clone(),
            ui.res.dig_io.uart_tx_en.clone(),
        ];
        let dir_highlight = self.highlighted.clone();
        let screen = ui.screen.clone();

        spawn(async move {
            dir_highlight.set(0).await;

            while let Some(ev) = button_events.next().await {
                let highlighted = dir_highlight.get().await.as_deref().copied().unwrap_or(0);

                if let ButtonEvent::ButtonOne(dur) = *ev {
                    if dur > LONG_PRESS {
                        let port = &dir_enables[highlighted as usize];
                        let state = port.get().await.as_deref().copied().unwrap_or(true);
                        port.set(!state).await;
                    } else {
                        dir_highlight.set((highlighted + 1) % 2).await;
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
