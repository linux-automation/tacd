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
    pub frame: [u8; 128 * 64 * 2],
}

impl Framebuffer {
    pub fn new(_: &str) -> Result<Self, ()> {
        Ok(Self {
            device: (),
            var_screen_info: VarScreenInfo {
                activate: 0,
                bits_per_pixel: 16,
                xres: 128,
                yres: 64,
            },
            fix_screen_info: FixScreenInfo { line_length: 256 },
            frame: [0; 128 * 64 * 2],
        })
    }

    pub fn put_var_screeninfo(_: &(), _: &VarScreenInfo) -> Result<(), ()> {
        Ok(())
    }
}
