//! Build script for rd-mediakit.
//!
//! On Windows, link the vendored MediaKit import library from
//! `crates/rd-analyze/native`. The release ships the import library
//! under the name `media_kit.dll.lib`; MSVC's linker searches for
//! `media_kit.lib` when handed `-l media_kit`, so the vendored file
//! is staged under that canonical name in OUT_DIR and both folders
//! are put on the link search path. On every other platform this
//! script does nothing: the crate compiles to an honest
//! "unavailable on this platform" stub and links nothing.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../rd-analyze/native/media_kit.dll.lib");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let native = manifest.join("../rd-analyze/native");
    let import_lib = native.join("media_kit.dll.lib");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let staged = out_dir.join("media_kit.lib");
    std::fs::copy(&import_lib, &staged).unwrap_or_else(|e| {
        panic!(
            "could not stage MediaKit import library from {}: {e}",
            import_lib.display()
        )
    });

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-search=native={}", native.display());
    println!("cargo:rustc-link-lib=dylib=media_kit");
}
