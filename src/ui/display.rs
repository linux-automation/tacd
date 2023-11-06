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

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use png::{BitDepth, ColorType, Encoder};

#[cfg(feature = "demo_mode")]
mod backend {
    use framebuffer::{FixScreeninfo, VarScreeninfo};

    pub(super) struct Framebuffer {
        pub device: (),
        pub var_screen_info: VarScreeninfo,
        pub fix_screen_info: FixScreeninfo,
        pub frame: [u8; 240 * 240 * 2],
    }

    impl Framebuffer {
        pub fn new(_: &str) -> Result<Self, ()> {
            Ok(Self {
                device: (),
                var_screen_info: VarScreeninfo {
                    bits_per_pixel: 16,
                    xres: 240,
                    yres: 240,
                    ..Default::default()
                },
                fix_screen_info: FixScreeninfo {
                    line_length: 480,
                    ..Default::default()
                },
                frame: [0; 240 * 240 * 2],
            })
        }

        pub fn put_var_screeninfo(_: &(), _: &VarScreeninfo) -> Result<(), ()> {
            Ok(())
        }
    }
}

#[cfg(not(feature = "demo_mode"))]
mod backend {
    pub(super) use framebuffer::*;
}

use backend::Framebuffer;

pub struct DisplayExclusive(Framebuffer);

pub struct Display {
    inner: Arc<Mutex<DisplayExclusive>>,
}

pub struct ScreenShooter {
    inner: Arc<Mutex<DisplayExclusive>>,
}

impl Display {
    pub fn new() -> Self {
        let mut fb = Framebuffer::new("/dev/fb0").unwrap();
        fb.var_screen_info.activate = 128; // FB_ACTIVATE_FORCE
        Framebuffer::put_var_screeninfo(&fb.device, &fb.var_screen_info).unwrap();

        let de = DisplayExclusive(fb);
        let inner = Arc::new(Mutex::new(de));

        Self { inner }
    }

    pub fn with_lock<F, R>(&self, cb: F) -> R
    where
        F: FnOnce(&mut DisplayExclusive) -> R,
    {
        cb(&mut self.inner.lock().unwrap())
    }

    pub fn clear(&self) {
        self.with_lock(|target| target.0.frame.iter_mut().for_each(|p| *p = 0x00));
    }

    pub fn screenshooter(&self) -> ScreenShooter {
        ScreenShooter {
            inner: self.inner.clone(),
        }
    }
}

impl ScreenShooter {
    pub fn as_png(&self) -> Vec<u8> {
        let (image, xres, yres) = {
            let fb = &self.inner.lock().unwrap().0;

            let bpp = (fb.var_screen_info.bits_per_pixel / 8) as usize;
            let xres = fb.var_screen_info.xres;
            let yres = fb.var_screen_info.yres;
            let res = (xres as usize) * (yres as usize);

            let image: Vec<u8> = (0..res)
                .map(|i| if fb.frame[i * bpp] != 0 { 0xff } else { 0 })
                .collect();

            (image, xres, yres)
        };

        let mut dst = Cursor::new(Vec::new());

        let mut writer = {
            let mut enc = Encoder::new(&mut dst, xres, yres);
            enc.set_color(ColorType::Grayscale);
            enc.set_depth(BitDepth::Eight);
            enc.write_header().unwrap()
        };

        writer.write_image_data(&image).unwrap();
        writer.finish().unwrap();

        dst.into_inner()
    }
}

impl DrawTarget for DisplayExclusive {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bpp = self.0.var_screen_info.bits_per_pixel / 8;
        let xres = self.0.var_screen_info.xres;
        let yres = self.0.var_screen_info.yres;
        let line_length = self.0.fix_screen_info.line_length;

        for Pixel(coord, color) in pixels {
            let x = coord.x as u32;
            let y = coord.y as u32;

            if x >= xres || y >= yres {
                continue;
            }

            let offset = line_length * y + bpp * x;

            for b in 0..bpp {
                self.0.frame[(offset + b) as usize] = match color {
                    BinaryColor::Off => 0x00,
                    BinaryColor::On => 0xff,
                }
            }
        }

        Ok(())
    }
}

impl OriginDimensions for DisplayExclusive {
    fn size(&self) -> Size {
        Size::new(self.0.var_screen_info.xres, self.0.var_screen_info.yres)
    }
}
