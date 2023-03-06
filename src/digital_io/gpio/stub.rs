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
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use std::iter::Iterator;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;
use async_std::sync::{Arc, Mutex};
use async_std::task::block_on;

static LINES: Mutex<Vec<(String, Arc<AtomicU8>)>> = Mutex::new(Vec::new());

pub struct LineHandle {
    name: String,
    val: Arc<AtomicU8>,
}

impl LineHandle {
    pub fn set_value(&self, val: u8) -> Result<(), ()> {
        println!("GPIO simulation set {} to {}", self.name, val);
        self.val.store(val, Ordering::Relaxed);
        Ok(())
    }
}

pub struct LineEvent(u8);

impl LineEvent {
    pub fn event_type(&self) -> EventType {
        match self.0 {
            0 => EventType::FallingEdge,
            _ => EventType::RisingEdge,
        }
    }
}

pub struct LineEventHandle {
    val: Arc<AtomicU8>,
    prev_val: u8,
}

impl LineEventHandle {
    pub fn get_value(&self) -> Result<u8, ()> {
        Ok(self.val.load(Ordering::Relaxed))
    }
}

impl Iterator for LineEventHandle {
    type Item = Result<LineEvent, ()>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let val = self.val.load(Ordering::Relaxed);

            if val != self.prev_val {
                self.prev_val = val;
                return Some(Ok(LineEvent(val)));
            }

            sleep(Duration::from_millis(100));
        }
    }
}

pub enum EventType {
    RisingEdge,
    FallingEdge,
}

#[allow(non_camel_case_types)]
pub enum EventRequestFlags {
    BOTH_EDGES,
}

pub enum LineRequestFlags {
    OUTPUT,
    INPUT,
}

pub struct FindDecoy {
    name: String,
    val: Arc<AtomicU8>,
}

impl FindDecoy {
    pub fn request(&self, _: LineRequestFlags, initial: u8, _: &str) -> Result<LineHandle> {
        self.val.store(initial, Ordering::Relaxed);

        Ok(LineHandle {
            name: self.name.clone(),
            val: self.val.clone(),
        })
    }

    pub fn events(
        &self,
        _: LineRequestFlags,
        _: EventRequestFlags,
        _: &str,
    ) -> Result<LineEventHandle> {
        Ok(LineEventHandle {
            val: self.val.clone(),
            prev_val: self.val.load(Ordering::Relaxed),
        })
    }

    #[cfg(test)]
    pub fn stub_get(&self) -> u8 {
        self.val.load(Ordering::Relaxed)
    }
}

pub fn find_line(name: &str) -> Result<FindDecoy> {
    let val = {
        let mut lines = block_on(LINES.lock());

        if let Some((_, v)) = lines.iter().find(|(n, _)| n == name) {
            v.clone()
        } else {
            let v = Arc::new(AtomicU8::new(0));
            lines.push((name.to_string(), v.clone()));
            v
        }
    };

    Ok(FindDecoy {
        name: name.to_string(),
        val: val,
    })
}
