// * ./src/qr.rs

use anyhow::Result;
use qrcode::QrCode;

pub fn generate_bytes_for_pixbuf(data: &str) -> Result<(Vec<u8>, i32, i32)> {
    let code = QrCode::new(data)?;
    let size = code.width();
    let scale = 6;
    let img_size = (size * scale) as i32;

    let rendered = code
        .render::<char>()
        .module_dimensions(scale as u32, scale as u32)
        .build();

    let mut rgb_bytes = Vec::with_capacity((img_size * img_size * 3) as usize);
    for line in rendered.lines() {
        for ch in line.chars() {
            let val = if ch == '\u{2588}' { 0u8 } else { 255u8 };
            rgb_bytes.push(val);
            rgb_bytes.push(val);
            rgb_bytes.push(val);
        }
    }

    Ok((rgb_bytes, img_size, img_size))
}
