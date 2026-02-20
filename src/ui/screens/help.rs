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
use embedded_graphics::prelude::Point;

use super::Display;
use super::widgets::*;
use super::{
    ActivatableScreen, ActiveScreen, AlertList, AlertScreen, Alerter, InputEvent, Screen, Ui,
};
use crate::broker::Topic;
use crate::watched_tasks::WatchedTasksBuilder;

const SCREEN_TYPE: AlertScreen = AlertScreen::Help;
const PAGES: &[&str] = &[
    "Hey there!
A short guide on how
this interface works:

Please long press the
lower button to
continue.
...",
    "...

Long presses on the
lower button perform
actions.

...",
    "...

Short presses on the
lower button toggle
between options.

...",
    "...

And the upper button
switches to the next
screen.

Press it to leave
this guide",
];

pub struct HelpScreen;

struct Active {
    widgets: WidgetContainer,
    up: Arc<Topic<bool>>,
    page: Arc<Topic<usize>>,
    show_help: Arc<Topic<bool>>,
}

impl HelpScreen {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        alerts: &Arc<Topic<AlertList>>,
        show_help: &Arc<Topic<bool>>,
    ) -> Result<Self> {
        let (mut show_help_events, _) = show_help.clone().subscribe_unbounded();
        let alerts = alerts.clone();

        wtb.spawn_task("screen-help-activator", async move {
            while let Some(show_help) = show_help_events.next().await {
                if show_help {
                    alerts.assert(AlertScreen::Help);
                } else {
                    alerts.deassert(AlertScreen::Help);
                }
            }

            Ok(())
        })?;

        Ok(Self)
    }
}

impl ActivatableScreen for HelpScreen {
    fn my_type(&self) -> Screen {
        Screen::Alert(SCREEN_TYPE)
    }

    fn activate(&mut self, ui: &Ui, display: Display) -> Box<dyn ActiveScreen> {
        display.with_lock(|target| draw_button_legend(target, "Action", "Leave"));

        let mut widgets = WidgetContainer::new(display);

        let up = Topic::anonymous(Some(false));
        let page = Topic::anonymous(Some(0));

        widgets.push(|display| {
            DynamicWidget::text(
                page.clone(),
                display,
                Point::new(8, 24),
                Box::new(|page| PAGES[*page].into()),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                up.clone(),
                display,
                Point::new(8, 200),
                Box::new(|up| match up {
                    false => "  Scroll up".into(),
                    true => "> Scroll up".into(),
                }),
            )
        });

        widgets.push(|display| {
            DynamicWidget::text(
                up.clone(),
                display,
                Point::new(8, 220),
                Box::new(|up| match up {
                    false => "> Scroll down".into(),
                    true => "  Scroll down".into(),
                }),
            )
        });

        let show_help = ui.res.setup_mode.show_help.clone();

        let active = Active {
            widgets,
            up,
            page,
            show_help,
        };

        Box::new(active)
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
            InputEvent::NextScreen => {
                self.show_help.set(false);
            }
            InputEvent::ToggleAction(_) => self.up.toggle(false),
            InputEvent::PerformAction(_) => {
                let up = self.up.try_get().unwrap_or(false);

                self.page.modify(|page| match (page.unwrap_or(0), up) {
                    (0, true) => Some(0),
                    (p, true) => Some(p - 1),
                    (3, false) => Some(3),
                    (p, false) => Some(p + 1),
                });
            }
        }
    }
}
