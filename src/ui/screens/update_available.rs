// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2022 Pengutronix e.K.
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
use serde::{Deserialize, Serialize};

use super::widgets::*;
use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, Display, InputEvent, Screen,
    Ui, row_anchor,
};
use crate::broker::Topic;
use crate::dbus::rauc::{Channel, Channels, UpdateRequest};
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::UpdateAvailable;

#[derive(Serialize, Deserialize, PartialEq, Clone)]
enum Highlight {
    Channel(usize),
    Dismiss,
}

impl Highlight {
    fn next(&self, num_channels: usize) -> Self {
        if num_channels == 0 {
            return Self::Dismiss;
        }

        match self {
            Self::Channel(ch) if (ch + 1) >= num_channels => Self::Dismiss,
            Self::Channel(ch) => Self::Channel(ch + 1),
            Self::Dismiss => Self::Channel(0),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Selection {
    channels: Vec<Channel>,
    highlight: Highlight,
}

impl Selection {
    fn new() -> Self {
        Self {
            channels: Vec::new(),
            highlight: Highlight::Dismiss,
        }
    }

    fn have_update(&self) -> bool {
        !self.channels.is_empty()
    }

    fn update_channels(&self, channels: Channels) -> Option<Self> {
        let channels: Vec<Channel> = channels
            .into_vec()
            .into_iter()
            .filter(|ch| {
                ch.bundle
                    .as_ref()
                    .map(|b| b.newer_than_installed)
                    .unwrap_or(false)
            })
            .collect();

        if channels == self.channels {
            return None;
        }

        let highlight = match self.highlight {
            Highlight::Channel(index) => {
                let name = &self.channels[index].name;

                match channels.iter().position(|ch| &ch.name == name) {
                    Some(idx) => Highlight::Channel(idx),
                    None => Highlight::Dismiss,
                }
            }
            Highlight::Dismiss => Highlight::Dismiss,
        };

        Some(Self {
            channels,
            highlight,
        })
    }

    fn toggle(self) -> Option<Self> {
        let num_channels = self.channels.len();
        let highlight = self.highlight.next(num_channels);

        if highlight != self.highlight {
            Some(Self {
                channels: self.channels,
                highlight,
            })
        } else {
            None
        }
    }

    fn perform(&self, alerts: &Arc<Topic<AlertList>>, install: &Arc<Topic<UpdateRequest>>) {
        match self.highlight {
            Highlight::Channel(ch) => {
                let req = UpdateRequest {
                    url: Some(self.channels[ch].url.clone()),
                };

                install.set(req);
            }
            Highlight::Dismiss => alerts.deassert(SCREEN_TYPE),
        }
    }
}

pub struct UpdateAvailableScreen {
    selection: Arc<Topic<Selection>>,
}

struct Active {
    widgets: WidgetContainer,
    alerts: Arc<Topic<AlertList>>,
    install: Arc<Topic<UpdateRequest>>,
    selection: Arc<Topic<Selection>>,
}

impl UpdateAvailableScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        channels: &Arc<Topic<Channels>>,
    ) -> Result<Self> {
        let (mut channels_events, _) = channels.clone().subscribe_unbounded();
        let alerts = alerts.clone();
        let selection = Topic::anonymous(Some(Selection::new()));
        let selection_task = selection.clone();

        wtb.spawn_task("screen-update-available-activator", async move {
            while let Some(channels) = channels_events.next().await {
                selection_task.modify(|sel| sel.unwrap().update_channels(channels));

                if selection_task.try_get().unwrap().have_update() {
                    alerts.assert(SCREEN_TYPE);
                } else {
                    alerts.deassert(SCREEN_TYPE);
                }
            }

            Ok(())
        })?;

        Ok(Self { selection })
    }
}

impl ActivatableScreen for UpdateAvailableScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        let mut widgets = WidgetContainer::new(display);

        widgets.push(|display| {
            DynamicWidget::new(
                self.selection.clone(),
                display,
                Box::new(move |sel, target| {
                    draw_button_legend(target, "Select", "-");

                    let ui_text_style: MonoTextStyle<BinaryColor> =
                        MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                    let num_updates = sel.channels.len();

                    let header = match num_updates {
                        0 => "There are no updates\navailable.",
                        1 => "There is an update\navailable.",
                        _ => "There are updates\navailable.",
                    };

                    Text::new(header, row_anchor(0), ui_text_style)
                        .draw(target)
                        .unwrap();

                    let sel_idx = match sel.highlight {
                        Highlight::Channel(idx) => idx,
                        Highlight::Dismiss => num_updates,
                    };

                    for (idx, ch) in sel.channels.iter().enumerate() {
                        let text = format!(
                            "{} Install {}",
                            if idx == sel_idx { ">" } else { " " },
                            ch.display_name,
                        );

                        Text::new(&text, row_anchor(idx as u8 + 3), ui_text_style)
                            .draw(target)
                            .unwrap();
                    }

                    let dismiss = match sel.highlight {
                        Highlight::Channel(_) => "  Dismiss",
                        Highlight::Dismiss => "> Dismiss",
                    };

                    Text::new(dismiss, row_anchor(num_updates as u8 + 3), ui_text_style)
                        .draw(target)
                        .unwrap();

                    // Don't bother tracking the actual bounding box and instead
                    // clear the whole screen on update.
                    Some(target.bounding_box())
                }),
            )
        });

        let alerts = ui.alerts.clone();
        let install = ui.res.rauc.install.clone();
        let selection = self.selection.clone();

        Box::new(Active {
            widgets,
            alerts,
            install,
            selection,
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
                self.selection
                    .modify(|selection| selection.and_then(|s| s.toggle()));
            }
            InputEvent::PerformAction(_) => {
                if let Some(selection) = self.selection.try_get() {
                    selection.perform(&self.alerts, &self.install);
                }
            }
        }
    }
}
