//! Embed the app icon and version info into claude-manager.exe.

#[path = "src/icons.rs"]
#[allow(dead_code)]
mod icons;

use std::io::Write;
use std::path::PathBuf;

/// Coral, the brand color.
const LOGO_RGB: u32 = 0xE2725B;

fn main() {
    println!("cargo:rerun-if-changed=src/icons.rs");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("app.ico");
    write_ico(&ico, &[16, 20, 24, 32, 40, 48, 64, 256]).expect("write app.ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().unwrap())
        .set("ProductName", "Claude Manager")
        .set("FileDescription", "Claude Manager")
        .set("CompanyName", "Claude Manager contributors")
        .set(
            "LegalCopyright",
            "MIT License. Not affiliated with Anthropic.",
        );
    res.compile().expect("compile Windows resources");
}

/// A multi-size .ico of 32-bit DIB images.
fn write_ico(path: &std::path::Path, sizes: &[u32]) -> std::io::Result<()> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| dib(s)).collect();
    let mut f = std::fs::File::create(path)?;
    let count = sizes.len() as u16;
    f.write_all(&[0, 0, 1, 0])?;
    f.write_all(&count.to_le_bytes())?;
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&images) {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        f.write_all(&[dim, dim, 0, 0])?;
        f.write_all(&1u16.to_le_bytes())?; // planes
        f.write_all(&32u16.to_le_bytes())?; // bpp
        f.write_all(&(img.len() as u32).to_le_bytes())?;
        f.write_all(&offset.to_le_bytes())?;
        offset += img.len() as u32;
    }
    for img in &images {
        f.write_all(img)?;
    }
    Ok(())
}

/// BITMAPINFOHEADER + bottom-up BGRA rows + an empty AND mask.
fn dib(size: u32) -> Vec<u8> {
    let rgba = icons::logo_rgba(size, LOGO_RGB);
    let mut v = Vec::new();
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&(size as i32).to_le_bytes());
    v.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&32u16.to_le_bytes());
    v.extend_from_slice(&[0u8; 24]); // compression, image size, resolution, colors
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            v.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    let mask_row = (size.div_ceil(32) * 4) as usize;
    v.extend(std::iter::repeat_n(0u8, mask_row * size as usize));
    v
}
