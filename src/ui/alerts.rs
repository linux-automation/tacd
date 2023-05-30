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

use serde::{Deserialize, Serialize};

use super::AlertScreen;
use crate::broker::Topic;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AlertList(Vec<AlertScreen>);

pub trait Alerter {
    fn assert(&self, screen: AlertScreen);
    fn deassert(&self, screen: AlertScreen);
}

impl AlertList {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn highest_priority(&self) -> Option<AlertScreen> {
        self.0.last().copied()
    }
}

impl Alerter for Topic<AlertList> {
    fn assert(&self, screen: AlertScreen) {
        self.modify(|list| {
            let mut list = list.unwrap();

            if list.0.iter().any(|s| s == &screen) {
                None
            } else {
                list.0.push(screen);
                list.0.sort();

                Some(list)
            }
        });
    }

    fn deassert(&self, screen: AlertScreen) {
        self.modify(|list| {
            let mut list = list.unwrap();

            if let Some(idx) = list.0.iter().position(|s| s == &screen) {
                list.0.remove(idx);
                Some(list)
            } else {
                None
            }
        });
    }
}
