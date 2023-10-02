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
use crate::temperatures::Warning;
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::OverTemperature;

pub struct OverTemperatureScreen;

struct Active {
    widgets: WidgetContainer,
}

impl OverTemperatureScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        warning: &Arc<Topic<Warning>>,
    ) -> Self {
        let (mut warning_events, _) = warning.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-overtemperature-activator", async move {
            while let Some(warning) = warning_events.next().await {
                match warning {
                    Warning::Okay => alerts.deassert(SCREEN_TYPE),
                    Warning::SocHigh | Warning::SocCritical => alerts.assert(SCREEN_TYPE),
                }
            }

            Ok(())
        });

        Self
    }
}

impl ActivatableScreen for OverTemperatureScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            Text::new("Temperature alert!", row_anchor(0), ui_text_style)
                .draw(target)
                .unwrap();

            Text::new(
                "TAC is overheating.\nProvide more airflow\nand check loads.",
                row_anchor(2),
                ui_text_style,
            )
            .draw(target)
            .unwrap();

            Text::new("SoC Temperature:", row_anchor(6), ui_text_style)
                .draw(target)
                .unwrap();
        });

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| {
            DynamicWidget::text_center(
                ui.res.temperatures.soc_temperature.clone(),
                display,
                Point::new(120, 210),
                Box::new(|meas: &Measurement| format!("{:-4.0} C", meas.value)),
            )
        });

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
