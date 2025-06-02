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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::MonoTextStyle, pixelcolor::BinaryColor, prelude::*, text::Text,
};

use super::widgets::*;
use super::{
    row_anchor, ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display,
    InputEvent, Screen, Ui,
};
use crate::broker::Topic;
use crate::measurement::Measurement;
use crate::usb_hub::{OverloadedPort, MAX_PORT_CURRENT, MAX_TOTAL_CURRENT};
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::UsbOverload;
const OFFSET_BAR: Point = Point::new(75, -14);
const OFFSET_VAL: Point = Point::new(145, 0);
const WIDTH_BAR: u32 = 65;
const HEIGHT_BAR: u32 = 18;

pub struct UsbOverloadScreen;

struct Active {
    widgets: WidgetContainer,
}

impl UsbOverloadScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        overload: &Arc<Topic<Option<OverloadedPort>>>,
    ) -> Result<Self> {
        let (mut overload_events, _) = overload.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-usb-overload-activator", async move {
            while let Some(overload) = overload_events.next().await {
                if overload.is_some() {
                    alerts.assert(SCREEN_TYPE)
                } else {
                    alerts.deassert(SCREEN_TYPE)
                }
            }

            Ok(())
        })?;

        Ok(Self)
    }
}

impl ActivatableScreen for UsbOverloadScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            // This screen can only be left by resolving the underlying issue
            draw_button_legend(target, "-", "-");

            Text::new(
                "USB Power Overload",
                row_anchor(0) - (row_anchor(1) - row_anchor(0)),
                ui_text_style,
            )
            .draw(target)
            .unwrap();

            Text::new(
                "Disconnect devices or\nuse a powered hub.",
                row_anchor(1),
                ui_text_style,
            )
            .draw(target)
            .unwrap();

            for (row, name) in &[(4, "Total"), (6, "Port 1"), (7, "Port 2"), (8, "Port 3")] {
                Text::new(name, row_anchor(*row), ui_text_style)
                    .draw(target)
                    .unwrap();
            }
        });

        let mut widgets = WidgetContainer::new(display);

        let ports = [
            (0, &ui.res.adc.usb_host_curr.topic, MAX_TOTAL_CURRENT),
            (2, &ui.res.adc.usb_host1_curr.topic, MAX_PORT_CURRENT),
            (3, &ui.res.adc.usb_host2_curr.topic, MAX_PORT_CURRENT),
            (4, &ui.res.adc.usb_host3_curr.topic, MAX_PORT_CURRENT),
        ];

        for (idx, current, max_current) in ports {
            let anchor_port = row_anchor(idx + 4);

            widgets.push(|display| {
                DynamicWidget::bar(
                    current.clone(),
                    display,
                    anchor_port + OFFSET_BAR,
                    WIDTH_BAR,
                    HEIGHT_BAR,
                    Box::new(move |meas: &Measurement| meas.value / max_current),
                )
            });

            widgets.push(|display| {
                DynamicWidget::text(
                    current.clone(),
                    display,
                    anchor_port + OFFSET_VAL,
                    Box::new(|meas: &Measurement| format!("{:>4.0}mA", meas.value * 1000.0)),
                )
            });
        }

        Box::new(Active { widgets })
    }
}

#[async_trait]
impl ActiveScreen for Active {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    async fn deactivate(mut self: Box<Self>) -> Display {
        self.widgets.destroy().await
    }

    fn input(&mut self, _ev: InputEvent) {}
}
