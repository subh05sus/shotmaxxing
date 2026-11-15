//! Links clang's runtime library into macOS builds. whisper.cpp's Metal code checks the macOS version with
//! `@available`, which clang turns into a call to `__isPlatformVersionAtLeast` when the program also supports older
//! versions: the release supports macOS 14, and Metal residency sets need 15. Rust's std has that function, but a
//! release built with LTO drops it because no Rust code calls it, and Rust links with `-nodefaultlibs`, so clang's own
//! copy isn't linked either, and the Apple silicon release didn't link. The library is only read for what nothing else
//! defines.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Some(dir) = clang_runtime() else {
        println!(
            "cargo:warning=clang's runtime library (libclang_rt.osx.a) wasn't found; a release build may not link"
        );
        return;
    };
    // Found again when the toolchain moves, as after an Xcode update.
    println!("cargo:rerun-if-changed={}", dir.join("libclang_rt.osx.a").display());
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static:-bundle=clang_rt.osx");
}

/// The folder where the active toolchain's clang keeps `libclang_rt.osx.a`.
fn clang_runtime() -> Option<PathBuf> {
    let out = Command::new("xcrun").args(["clang", "--print-resource-dir"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let dir = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim()).join("lib/darwin");
    dir.join("libclang_rt.osx.a").is_file().then_some(dir)
}
