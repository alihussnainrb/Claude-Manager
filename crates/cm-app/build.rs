//! Embed the app icon and version info into claude-manager.exe.

#[path = "src/icons.rs"]
#[allow(dead_code)]
mod icons;

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
    let images: Vec<(u32, Vec<u8>)> = [16, 20, 24, 32, 40, 48, 64, 256]
        .into_iter()
        .map(|s| (s, icons::logo_rgba(s, LOGO_RGB)))
        .collect();
    std::fs::write(&ico, icons::ico_bytes(&images)).expect("write app.ico");

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
