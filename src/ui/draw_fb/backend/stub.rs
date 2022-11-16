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

pub struct VarScreenInfo {
    pub activate: u8,
    pub bits_per_pixel: u32,
    pub xres: u32,
    pub yres: u32,
}

pub struct FixScreenInfo {
    pub line_length: u32,
}

pub struct Framebuffer {
    pub device: (),
    pub var_screen_info: VarScreenInfo,
    pub fix_screen_info: FixScreenInfo,
    pub frame: [u8; 240 * 240 * 2],
}

impl Framebuffer {
    pub fn new(_: &str) -> Result<Self, ()> {
        Ok(Self {
            device: (),
            var_screen_info: VarScreenInfo {
                activate: 0,
                bits_per_pixel: 16,
                xres: 240,
                yres: 240,
            },
            fix_screen_info: FixScreenInfo { line_length: 480 },
            frame: [0; 240 * 240 * 2],
        })
    }

    pub fn put_var_screeninfo(_: &(), _: &VarScreenInfo) -> Result<(), ()> {
        Ok(())
    }
}
