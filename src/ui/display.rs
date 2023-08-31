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

    pub struct Framebuffer {
        pub device: (),
        pub var_screen_info: VarScreeninfo,
        pub fix_screen_info: FixScreeninfo,
        pub frame: [u8; 240 * 240 * 4],
    }

    impl Framebuffer {
        pub fn new(_: &str) -> Result<Self, ()> {
            Ok(Self {
                device: (),
                var_screen_info: VarScreeninfo {
                    bits_per_pixel: 32,
                    xres: 240,
                    yres: 240,
                    ..Default::default()
                },
                fix_screen_info: FixScreeninfo {
                    line_length: 240 * 4,
                    ..Default::default()
                },
                frame: [0; 240 * 240 * 4],
            })
        }

        pub fn put_var_screeninfo(_: &(), _: &VarScreeninfo) -> Result<(), ()> {
            Ok(())
        }
    }
}

#[cfg(not(feature = "demo_mode"))]
mod backend {
    pub use framebuffer::*;
}

use backend::Framebuffer;

const BACKGROUND: &[(u8, u8, u8)] = include!(concat!(env!("OUT_DIR"), "/background.rs"));

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
        self.with_lock(|target| target.clear(BinaryColor::Off).unwrap());
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

            assert!(fb.var_screen_info.bits_per_pixel == 32);
            let xres = fb.var_screen_info.xres as usize;
            let yres = fb.var_screen_info.yres as usize;

            let mut image = vec![0; xres * yres * 3];

            for y in 0..yres {
                for x in 0..xres {
                    let idx = y * xres + x;

                    image[idx * 3] = fb.frame[idx * 4 + 2];
                    image[idx * 3 + 1] = fb.frame[idx * 4 + 1];
                    image[idx * 3 + 2] = fb.frame[idx * 4];
                }
            }

            (image, xres, yres)
        };

        let mut dst = Cursor::new(Vec::new());

        let mut writer = {
            let mut enc = Encoder::new(&mut dst, xres as u32, yres as u32);
            enc.set_color(ColorType::Rgb);
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
        assert!(self.0.var_screen_info.bits_per_pixel == 32);
        let xres = self.0.var_screen_info.xres;
        let yres = self.0.var_screen_info.yres;
        let line_length = self.0.fix_screen_info.line_length;

        for Pixel(coord, color) in pixels {
            let x = coord.x as u32;
            let y = coord.y as u32;

            if x >= xres || y >= yres {
                continue;
            }

            let offset_bg = (y * xres + x) as usize;
            let offset_fb = (line_length * y + 4 * x) as usize;

            let rgb = match color {
                BinaryColor::Off => BACKGROUND[offset_bg],
                BinaryColor::On => (255, 255, 255),
            };

            self.0.frame[offset_fb] = rgb.2;
            self.0.frame[offset_fb + 1] = rgb.1;
            self.0.frame[offset_fb + 2] = rgb.0;
        }

        Ok(())
    }
}

impl OriginDimensions for DisplayExclusive {
    fn size(&self) -> Size {
        Size::new(self.0.var_screen_info.xres, self.0.var_screen_info.yres)
    }
}
