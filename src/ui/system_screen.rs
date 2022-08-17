use async_std::prelude::*;
use async_std::task::spawn;
use async_trait::async_trait;

use embedded_graphics::prelude::*;

use crate::adc::Measurement;
use crate::broker::{Native, SubscriptionHandle};
use crate::dbus::LinkInfo;

use super::widgets::*;
use super::{draw_border, ButtonEvent, MountableScreen, Screen, Ui};

const SCREEN_TYPE: Screen = Screen::System;

pub struct SystemScreen {
    widgets: Vec<Box<dyn AnyWidget>>,
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
}

impl SystemScreen {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for SystemScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        draw_border("System Status", SCREEN_TYPE, &ui.draw_target).await;

        self.widgets.push(Box::new(
            DynamicWidget::locator(ui.locator_dance.clone(), ui.draw_target.clone()).await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.temperatures.soc_temperature.clone(),
                ui.draw_target.clone(),
                Point::new(0, 26),
                Box::new(|meas: &Measurement| format!("SoC:    {:.0}C", meas.value)),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.dbus.network.uplink_interface.clone(),
                ui.draw_target.clone(),
                Point::new(0, 36),
                Box::new(|info: &LinkInfo| match info.carrier {
                    true => format!("Uplink: {}MBit/s", info.speed),
                    false => "Uplink: Down".to_string(),
                }),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.dbus.network.dut_interface.clone(),
                ui.draw_target.clone(),
                Point::new(0, 46),
                Box::new(|info: &LinkInfo| match info.carrier {
                    true => format!("DUT:    {}MBit/s", info.speed),
                    false => "DUT:    Down".to_string(),
                }),
            )
            .await,
        ));

        self.widgets.push(Box::new(
            DynamicWidget::text(
                ui.res.dbus.network.bridge_interface.clone(),
                ui.draw_target.clone(),
                Point::new(0, 56),
                Box::new(|ips: &Vec<String>| {
                    let ip = ips.get(0).map(|s| s.as_str()).unwrap_or("-");
                    format!("IP: {}", ip)
                }),
            )
            .await,
        ));

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        let screen = ui.screen.clone();

        spawn(async move {
            while let Some(ev) = button_events.next().await {
                screen
                    .set(match *ev {
                        ButtonEvent::ButtonOne(_) => Screen::RebootConfirm,
                        ButtonEvent::ButtonTwo(_) => SCREEN_TYPE.next(),
                    })
                    .await
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
