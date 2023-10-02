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
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::IoBusHealth;

pub struct IoBusHealthScreen;

struct Active {
    widgets: WidgetContainer,
    alerts: Arc<Topic<AlertList>>,
}

impl IoBusHealthScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        supply_fault: &Arc<Topic<bool>>,
    ) -> Self {
        let (mut supply_fault_events, _) = supply_fault.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-iobus-health-activator", async move {
            while let Some(fault) = supply_fault_events.next().await {
                if fault {
                    alerts.assert(SCREEN_TYPE);
                } else {
                    alerts.deassert(SCREEN_TYPE);
                }
            }

            Ok(())
        });

        Self
    }
}

impl ActivatableScreen for IoBusHealthScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            Text::new(
                "IOBus supply overload",
                row_anchor(0) - (row_anchor(1) - row_anchor(0)),
                ui_text_style,
            )
            .draw(target)
            .unwrap();

            Text::new(
                "The IOBus supply is\noverloaded by a short\nor too many devices.",
                row_anchor(1),
                ui_text_style,
            )
            .draw(target)
            .unwrap();

            Text::new("> Dismiss", row_anchor(8), ui_text_style)
                .draw(target)
                .unwrap();
        });

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.adc.iobus_volt.topic.clone(),
                display,
                row_anchor(5),
                Box::new(|meas: &Measurement| format!("  {:-6.2}V /  12V", meas.value)),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.adc.iobus_curr.topic.clone(),
                display,
                row_anchor(6),
                Box::new(|meas: &Measurement| format!("  {:-6.2}A / 0.2A", meas.value)),
            )
        });

        let alerts = ui.alerts.clone();

        Box::new(Active { widgets, alerts })
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

    fn input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::NextScreen | InputEvent::ToggleAction(_) => {}
            InputEvent::PerformAction(_) => {
                self.alerts.deassert(SCREEN_TYPE);
            }
        }
    }
}
