// File: qr.rs
// Location: /src/qr.rs

use anyhow::Result;
use qrcode::QrCode;
use qrcode::Color;
use image::{Luma, ImageBuffer};

pub fn generate_bytes_for_pixbuf(data: &str) -> Result<(Vec<u8>, i32, i32)> {
    let code = QrCode::new(data)?;
    let size = code.width() as usize;
    let scale = 4;
    let img_size = size * scale;

    let mut img = ImageBuffer::new(img_size as u32, img_size as u32);

    for y in 0..img_size {
        for x in 0..img_size {
            img.put_pixel(x as u32, y as u32, Luma([255]));
        }
    }

    for y in 0..size {
        for x in 0..size {
            if code[(x, y)] == Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(
                            (x * scale + dx) as u32,
                            (y * scale + dy) as u32,
                            Luma([0]),
                        );
                    }
                }
            }
        }
    }

    let width = img.width() as i32;
    let height = img.height() as i32;

    let mut rgb_bytes = Vec::with_capacity((width * height * 3) as usize);
    for pixel in img.pixels() {
        let val = pixel[0];
        rgb_bytes.push(val);
        rgb_bytes.push(val);
        rgb_bytes.push(val);
    }

    Ok((rgb_bytes, width, height))
}