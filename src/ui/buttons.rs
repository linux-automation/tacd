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

use std::time::Duration;

use async_std::sync::Arc;
use async_std::task::spawn_blocking;
use serde::{Deserialize, Serialize};

use crate::broker::Topic;

pub const LONG_PRESS: Duration = Duration::from_millis(750);

#[cfg(feature = "demo_mode")]
mod evd {
    use evdev::FetchEventsSynced;
    pub use evdev::{EventType, InputEventKind, Key};

    pub struct Device {}

    impl Device {
        pub fn open(_path: &'static str) -> Result<Self, ()> {
            Ok(Self {})
        }

        pub fn fetch_events(&mut self) -> Result<FetchEventsSynced, ()> {
            loop {
                std::thread::park()
            }
        }
    }
}

#[cfg(not(feature = "demo_mode"))]
mod evd {
    pub use evdev::*;
}

use evd::{Device, EventType, InputEventKind, Key};

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum Button {
    Upper,
    Lower,
}

impl Button {
    fn from_id(id: usize) -> Self {
        match id {
            0 => Button::Upper,
            1 => Button::Lower,
            _ => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum PressDuration {
    Short,
    Long,
}

impl PressDuration {
    fn from_duration(d: Duration) -> Self {
        if d >= LONG_PRESS {
            Self::Long
        } else {
            Self::Short
        }
    }
}

// There are certain actions that we only allow when they are performed
// on the local ui of the device, not from the web interface.
// E.g. going back to setup mode.
// The #[default] together with the serde(skip) below prevents the web ui
// from ever being able to simulate a local button press.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum Source {
    Local,
    #[default]
    Web,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum ButtonEvent {
    Press {
        btn: Button,
        #[serde(skip)]
        src: Source,
    },
    Release {
        btn: Button,
        dur: PressDuration,
        #[serde(skip)]
        src: Source,
    },
}

impl ButtonEvent {
    fn press_from_id(id: usize) -> Self {
        ButtonEvent::Press {
            btn: Button::from_id(id),
            src: Source::Local,
        }
    }

    fn release_from_id_duration(id: usize, duration: Duration) -> Self {
        ButtonEvent::Release {
            btn: Button::from_id(id),
            dur: PressDuration::from_duration(duration),
            src: Source::Local,
        }
    }
}

/// Spawn a thread that blockingly reads user input and pushes them into
/// a broker framework topic.
pub fn handle_buttons(path: &'static str, topic: Arc<Topic<ButtonEvent>>) {
    use super::*;

    spawn_blocking(move || {
        let mut device = Device::open(path).unwrap();
        let mut start_time = [None, None];

        loop {
            for ev in device.fetch_events().unwrap() {
                if ev.event_type() != EventType::KEY {
                    continue;
                }

                let id = match ev.kind() {
                    InputEventKind::Key(Key::KEY_HOME) => 0,
                    InputEventKind::Key(Key::KEY_ESC) => 1,
                    _ => continue,
                };

                if ev.value() == 0 {
                    // Button release -> send event
                    if let Some(start) = start_time[id].take() {
                        if let Ok(duration) = ev.timestamp().duration_since(start) {
                            let button_event = ButtonEvent::release_from_id_duration(id, duration);
                            topic.set(button_event);
                        }
                    }
                } else {
                    // Button press -> register start time and send event
                    start_time[id] = Some(ev.timestamp());
                    topic.set(ButtonEvent::press_from_id(id));
                }
            }
        }
    });
}
