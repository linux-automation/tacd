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
use serde::{Deserialize, Serialize};

use super::widgets::*;
use super::{
    row_anchor, ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display,
    InputEvent, Screen, Ui,
};
use crate::broker::Topic;
use crate::dut_power::{OutputRequest, OutputState};
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::PowerFail;

pub struct PowerFailScreen;

#[derive(Serialize, Deserialize, Clone)]
enum Highlight {
    TurnOn,
    KeepOff,
}

impl Highlight {
    fn next(&self) -> Self {
        match self {
            Self::TurnOn => Self::KeepOff,
            Self::KeepOff => Self::TurnOn,
        }
    }
}

struct Active {
    widgets: WidgetContainer,
    highlight: Arc<Topic<Highlight>>,
    request: Arc<Topic<OutputRequest>>,
}

impl PowerFailScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        out_state: &Arc<Topic<OutputState>>,
    ) -> Self {
        let (mut out_state_events, _) = out_state.clone().subscribe_unbounded();

        let alerts = alerts.clone();

        wtb.spawn_task("screen-power-fail-activator", async move {
            while let Some(state) = out_state_events.next().await {
                match state {
                    OutputState::On | OutputState::Off | OutputState::OffFloating => {
                        alerts.deassert(SCREEN_TYPE)
                    }
                    OutputState::InvertedPolarity
                    | OutputState::OverCurrent
                    | OutputState::OverVoltage
                    | OutputState::RealtimeViolation => alerts.assert(SCREEN_TYPE),
                    OutputState::Changing => {}
                }
            }

            Ok(())
        });

        Self
    }
}

impl ActivatableScreen for PowerFailScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let ui_text_style: MonoTextStyle<BinaryColor> =
            MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

        display.with_lock(|target| {
            Text::new(
                "DUT Power error",
                row_anchor(0) - (row_anchor(1) - row_anchor(0)),
                ui_text_style,
            )
            .draw(target)
            .unwrap();
        });

        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| {
            DynamicWidget::text(
                ui.res.dut_pwr.state.clone(),
                display,
                row_anchor(2),
                Box::new(|state: &OutputState| {
                    let msg = match state {
                        OutputState::On | OutputState::Off | OutputState::OffFloating => {
                            "The error was resolved"
                        }
                        OutputState::InvertedPolarity => {
                            "Output disabled due to\ninverted polarity."
                        }
                        OutputState::OverCurrent => "DUT powered off due to\nan overcurrent event.",
                        OutputState::OverVoltage => "DUT powered off due to\nan overvoltage event.",
                        OutputState::RealtimeViolation => {
                            "Output disabled due to\na realtime violation."
                        }
                        OutputState::Changing => "",
                    };

                    msg.to_string()
                }),
            )
        });

        let highlight = Topic::anonymous(Some(Highlight::KeepOff));

        widgets.push(|display| {
            DynamicWidget::text(
                highlight.clone(),
                display,
                row_anchor(6),
                Box::new(|highlight: &Highlight| {
                    let msg = match highlight {
                        Highlight::TurnOn => "> Turn output back on\n  Keep output off",
                        Highlight::KeepOff => "  Turn output back on\n> Keep output off",
                    };

                    msg.to_string()
                }),
            )
        });

        let request = ui.res.dut_pwr.request.clone();

        Box::new(Active {
            widgets,
            highlight,
            request,
        })
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
            InputEvent::NextScreen => {}
            InputEvent::ToggleAction(_) => {
                self.highlight
                    .modify(|highlight| highlight.map(|s| s.next()));
            }
            InputEvent::PerformAction(_) => match self.highlight.try_get() {
                Some(Highlight::TurnOn) => self.request.set(OutputRequest::On),
                Some(Highlight::KeepOff) => self.request.set(OutputRequest::Off),
                None => {}
            },
        }
    }
}
