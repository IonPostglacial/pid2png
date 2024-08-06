#![no_std]

use core::{panic::PanicInfo, slice};

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[derive(Clone, Copy)]
struct ImageFlags { flags: u32 }

#[link(wasm_import_module = "env")]
extern "C" {
    fn get_pid_data_u8(offset: u32) -> u8;
    fn get_pid_data_u32_le(offset: u32) -> u32;
    fn get_pid_data_i32_le(offset: u32) -> i32;
    fn alloc(size: u32) -> *mut u8;
}

struct Buffer {
    data: &'static mut [u8]
}

impl Buffer {
    fn new(size: usize) -> Buffer {
        Buffer { 
            data:  unsafe { slice::from_raw_parts_mut(alloc(size as u32), size) }
        }
    }

    fn write_u8(&mut self, n: usize, b: u8) {
        self.data[n] = b;
    }

    fn write_u32_le(&mut self, n: usize, u: u32) {
        let bytes = u.to_le_bytes();
        for i in 0..4 {
            self.data[n + i] = bytes[i];
        }
    }
}

struct OutputImage {
    buffer: Buffer,
}

impl OutputImage {
    fn from_canvas_with_dimensions(width: u32, height: u32) -> OutputImage {
        let mut data = Buffer::new(4 * (2 + width * height) as usize);
        data.write_u32_le(0, width);
        data.write_u32_le(4, height);
        OutputImage { buffer: data }
    }

    fn set_pixel(&mut self, px: usize, r: u8, g: u8, b: u8, a: u8) {
        let i = 8 + px * 4;
        self.buffer.write_u8(i, r);
        self.buffer.write_u8(i + 1, g);
        self.buffer.write_u8(i + 2, b);
        self.buffer.write_u8(i + 3, a);
    }
}

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8
}

struct PidDataCursor {
    offset: u32,
}

impl PidDataCursor {
    fn next_u8(&mut self) -> u8 {
        let output = unsafe { get_pid_data_u8(self.offset) };
        self.offset += 1;
        output
    }

    fn next_u32_le(&mut self) -> u32 {
        let output = unsafe { get_pid_data_u32_le(self.offset) };
        self.offset += 4;
        output
    }

    fn next_i32_le(&mut self) -> i32 {
        let output = unsafe { get_pid_data_i32_le(self.offset) };
        self.offset += 4;
        output
    }
}

impl ImageFlags {
    fn use_transparency(&self) -> bool {
        self.flags & 0x01 != 0
    }

    fn use_video_memory(&self) -> bool {
        self.flags & 0x02 != 0
    }

    fn use_system_memory(&self) -> bool {
        self.flags & 0x04 != 0
    }

    fn is_fliped_horizontally(&self) -> bool {
        self.flags & 0x08 != 0
    }

    fn is_fliped_vertically(&self) -> bool {
        self.flags & 0x10 != 0
    }

    fn compression_method(&self) -> CompressionMethod {
        if self.flags & 0x20 == 0 {
            CompressionMethod::Default
        } else {
            CompressionMethod::RunLengthEncoding
        }
    }

    fn has_lights(&self) -> bool {
        self.flags & 0x40 != 0
    }

    fn has_palette(&self) -> bool {
        self.flags & 0x80 != 0
    }
}

struct PidImage {
    id: i32,
    flags: ImageFlags,
    width: u32,
    height: u32,
    user_values: [i32; 4],
    pixels: &'static [u8],
    palette: Option<[Rgb; 256]>,
}

enum CompressionMethod { Default, RunLengthEncoding }

fn decompress_default(data: &mut PidDataCursor, pixels: &mut Buffer, pixels_count: usize) {
    let mut pixel = 0;
    while pixel < pixels_count {
        let n: u8;
        let b: u8;
        let a = data.next_u8();
        if a > 192 {
            n = a - 192;
            b = data.next_u8();
        } else {
            n = 1;
            b = a;
        }
        for _ in 0..n {
            pixels.write_u8(pixel, b);
            pixel += 1;
        }
    }
}

fn decompress_run_length_encoding(data: &mut PidDataCursor, pixels: &mut Buffer, pixels_count: usize) {
    let mut pixel = 0;
    while pixel < pixels_count {
        let a = data.next_u8();
        if a > 128 {
            let j = a - 128;
            for _ in 0..j {
                pixels.write_u8(pixel, 0);
                pixel += 1;
            }
        } else {
            for _ in 0..a {
                let b = data.next_u8();
                pixels.write_u8(pixel, b);
                pixel += 1;
            }
        }
    }
}

fn decode_pid() -> PidImage {
    let mut cur = PidDataCursor { offset: 0 };
    let id = cur.next_i32_le();

    // test
    let flags = ImageFlags { flags: cur.next_u32_le() };
    let width = cur.next_u32_le();
    let height = cur.next_u32_le();
    // end test
    let mut user_values: [i32; 4] = [0; 4];
    user_values[0] = cur.next_i32_le();
    user_values[1] = cur.next_i32_le();
    user_values[2] = cur.next_i32_le();
    user_values[3] = cur.next_i32_le();
    let pixels_count = (width * height) as usize;
    let mut pixels = Buffer::new(pixels_count);

    match flags.compression_method() {
        CompressionMethod::Default => decompress_default(&mut cur, &mut pixels, pixels_count),
        CompressionMethod::RunLengthEncoding => decompress_run_length_encoding(&mut cur, &mut pixels, pixels_count),
    }

    let palette = if flags.has_palette() {
        let mut p: [Rgb; 256] = [Rgb { r: 0, g: 0, b: 0}; 256];
        for c in &mut p {
            c.r = cur.next_u8();
            c.g = cur.next_u8();
            c.b = cur.next_u8();
        }
        Some(p)
    } else {
        None
    };
    
    PidImage { id, flags, width, height, user_values, pixels: pixels.data, palette }
}

#[export_name = "write_pid_to_canvas_image_data"]
pub extern "C" fn write_pid_to_canvas_image_data() -> *mut u8 {
    let img = decode_pid();
    let mut image = OutputImage::from_canvas_with_dimensions(img.width, img.height);
    let pix_count = img.width * img.height;
    if let Some(palette) = img.palette {
        for i in 0..pix_count {
            let pixel = img.pixels[i as usize];
            if img.flags.use_transparency() && pixel == 0 {
                image.set_pixel(i as usize, 0, 0, 0, 0);
            } else {
                let color = palette[pixel as usize];
                image.set_pixel(i as usize, color.r, color.g, color.b, 255);
            }
        }
    }
    image.buffer.data.as_mut_ptr()
}
