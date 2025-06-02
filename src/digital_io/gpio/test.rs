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

use std::cell::RefCell;
use std::ops::BitOr;
use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::Result;
use async_std::sync::Arc;

std::thread_local! {
    static LINES: RefCell<Vec<(String, Arc<AtomicU8>)>> = const { RefCell::new(Vec::new()) };
}

pub struct LineHandle {
    name: String,
    val: Arc<AtomicU8>,
}

impl LineHandle {
    pub fn set_value(&self, val: u8) -> Result<()> {
        println!("GPIO simulation set {} to {}", self.name, val);
        self.val.store(val, Ordering::Relaxed);
        Ok(())
    }
}

#[allow(clippy::upper_case_acronyms, non_camel_case_types)]
#[derive(Clone)]
pub enum LineRequestFlags {
    OUTPUT,
    OPEN_DRAIN,
}

impl BitOr for LineRequestFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::OPEN_DRAIN, res) | (res, Self::OPEN_DRAIN) => res,
            _ => unimplemented!(),
        }
    }
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

    pub fn stub_get(&self) -> u8 {
        self.val.load(Ordering::Relaxed)
    }
}

pub fn find_line(name: &str) -> Option<FindDecoy> {
    let val = LINES.with_borrow_mut(|lines| {
        if let Some((_, v)) = lines.iter().find(|(n, _)| n == name) {
            v.clone()
        } else {
            let v = Arc::new(AtomicU8::new(0));
            lines.push((name.to_string(), v.clone()));
            v
        }
    });

    Some(FindDecoy {
        name: name.to_string(),
        val,
    })
}
