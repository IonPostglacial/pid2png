use std::{env::args, fs, io::{Cursor, Read}};
use bytes::buf::Buf;
use image::{ImageBuffer, Pixel, Rgb, Rgba};

#[derive(Debug, Clone, Copy)]
struct ImageFlags { flags: u32 }

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

#[derive(Debug)]
struct PidImage {
    id: i32,
    flags: ImageFlags,
    width: u32,
    height: u32,
    user_values: [i32; 4],
    pixels: Vec<u8>,
    palette: Option<[Rgb<u8>; 256]>,
}

#[derive(Debug)]
enum CompressionMethod { Default, RunLengthEncoding }

fn decompress_default(data: &mut Cursor<&[u8]>, pixels: &mut Vec<u8>, pixels_count: usize) {
    while pixels.len() < pixels_count {
        let n: u8;
        let b: u8;
        let a = data.get_u8();
        if a > 192 {
            n = a - 192;
            b = data.get_u8();
        } else {
            n = 1;
            b = a;
        }
        for _ in 0..n {
            pixels.push(b);
        }
    }
}

fn decompress_run_length_encoding(data: &mut Cursor<&[u8]>, pixels: &mut Vec<u8>, pixels_count: usize) {
    while pixels.len() < pixels_count {
        let a = data.get_u8();
        if a > 128 {
            let j = a - 128;
            for _ in 0..j {
                pixels.push(0);
            }
        } else {
            for _ in 0..a {
                let b = data.get_u8();
                pixels.push(b);
            }
        }
    }
}

fn decode_pid(pid_data: &[u8]) -> PidImage {
    let mut cur = Cursor::new(pid_data);
    let id = cur.get_i32_le();
    let flags = ImageFlags { flags: cur.get_u32_le() };
    let width = cur.get_u32_le();
    let height = cur.get_u32_le();
    let mut user_values: [i32; 4] = [0; 4];
    user_values[0] = cur.get_i32();
    user_values[1] = cur.get_i32();
    user_values[2] = cur.get_i32();
    user_values[3] = cur.get_i32();
    let pixels_count = (width * height) as usize;
    let mut pixels = Vec::<u8>::with_capacity(pixels_count);

    match flags.compression_method() {
        CompressionMethod::Default => decompress_default(&mut cur, &mut pixels, pixels_count),
        CompressionMethod::RunLengthEncoding => decompress_run_length_encoding(&mut cur, &mut pixels, pixels_count),
    }

    let palette = if flags.has_palette() {
        let mut p: [Rgb<u8>; 256] = [Rgb::<u8>([0; 3]); 256];
        for c in &mut p {
            cur.read_exact(&mut c.0).expect("palette to be complete");
        }
        Some(p)
    } else {
        None
    };
    
    PidImage { id, flags, width, height, user_values, pixels, palette }
}

fn pid_image_to_image_buffer(img: &PidImage) -> ImageBuffer::<Rgba<u8>, Vec<u8>> {
    let mut output = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(img.width, img.height);
    if let Some(palette) = img.palette {
        for y in 0..img.height {
            for x in 0..img.width {
                let i = (y * img.width + x) as usize;
                let pixel = img.pixels[i];
                let color = if img.flags.use_transparency() && pixel == 0 {
                    Rgba::<u8>([0; 4])
                } else {
                    palette[pixel as usize].to_rgba()
                };
                output.put_pixel(x, y, color);
            }
        }
    }
    output
}

fn main() {
    let mut args = args();
    if args.len() < 3 {
        println!("Please provide 2 arguments: input file path and output file path.");
        return;
    }
    args.next();
    let pid_data = fs::read(args.next().unwrap()).expect("file to exist");
    let img = decode_pid(&pid_data);
    let output = pid_image_to_image_buffer(&img);
    output.save(args.next().unwrap()).expect("saving image to succeed");
}
