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

use super::ConnectionBuilder;

pub struct Tacd {}

#[cfg(not(feature = "stub_out_dbus"))]
#[zbus::dbus_interface(name = "de.pengutronix.tacd1")]
impl Tacd {
    fn get_version(&mut self) -> String {
        std::env!("VERSION_STRING").to_string()
    }
}

impl Tacd {
    pub fn new() -> Self {
        Self {}
    }

    pub fn serve(self, cb: ConnectionBuilder) -> ConnectionBuilder {
        cb.serve_at("/de/pengutronix/tacd", self).unwrap()
    }
}
