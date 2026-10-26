fn main() {
    link_clang_runtime();
    tauri_build::build()
}

/// whisper.cpp's Metal backend and ONNX Runtime's CoreML provider check the OS
/// version with Objective-C `@available`, which calls
/// `__isPlatformVersionAtLeast`. Rust's standard library defines it, but the
/// release profile's fat LTO drops that copy before the native libraries are
/// linked, so release builds also link the compiler runtime that provides it.
fn link_clang_runtime() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let dir = std::process::Command::new("clang")
        .arg("--print-runtime-dir")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    if dir.is_empty() {
        println!("cargo:warning=clang runtime directory not found; release builds may fail to link");
        return;
    }
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
