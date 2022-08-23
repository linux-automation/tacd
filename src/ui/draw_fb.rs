use std::io::Cursor;

use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use png::{BitDepth, ColorType, Encoder};

#[cfg(feature = "stub_out_display")]
mod backend {
    mod stub;
    pub use stub::*;
}

#[cfg(not(feature = "stub_out_display"))]
mod backend {
    pub use framebuffer::*;
}

use backend::Framebuffer;

pub struct FramebufferDrawTarget {
    fb: Framebuffer,
}

impl FramebufferDrawTarget {
    pub fn new() -> FramebufferDrawTarget {
        let mut fb = Framebuffer::new("/dev/fb0").unwrap();
        fb.var_screen_info.activate = 128; // FB_ACTIVATE_FORCE
        Framebuffer::put_var_screeninfo(&fb.device, &fb.var_screen_info).unwrap();

        FramebufferDrawTarget { fb }
    }

    pub fn clear(&mut self) {
        self.fb.frame.iter_mut().for_each(|p| *p = 0x00);
    }

    pub fn as_png(&self) -> Vec<u8> {
        let mut dst = Cursor::new(Vec::new());

        let bpp = (self.fb.var_screen_info.bits_per_pixel / 8) as usize;
        let xres = self.fb.var_screen_info.xres;
        let yres = self.fb.var_screen_info.yres;
        let res = (xres as usize) * (yres as usize);

        let image: Vec<u8> = (0..res)
            .map(|i| if self.fb.frame[i * bpp] != 0 { 0xff } else { 0 })
            .collect();

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

impl DrawTarget for FramebufferDrawTarget {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bpp = self.fb.var_screen_info.bits_per_pixel / 8;
        let xres = self.fb.var_screen_info.xres;
        let yres = self.fb.var_screen_info.yres;
        let line_length = self.fb.fix_screen_info.line_length;

        for Pixel(coord, color) in pixels {
            let x = coord.x as u32;
            let y = coord.y as u32;

            if x >= xres || y >= yres {
                continue;
            }

            let offset = line_length * y + bpp * x;

            for b in 0..bpp {
                self.fb.frame[(offset + b) as usize] = match color {
                    BinaryColor::Off => 0x00,
                    BinaryColor::On => 0xff,
                }
            }
        }

        Ok(())
    }
}

impl OriginDimensions for FramebufferDrawTarget {
    fn size(&self) -> Size {
        Size::new(self.fb.var_screen_info.xres, self.fb.var_screen_info.yres)
    }
}
