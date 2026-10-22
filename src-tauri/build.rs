fn main() {
    // Tests that create a Tauri app (update.rs diagnostic): the Windows common controls
    // manifest, like the exe (otherwise STATUS_ENTRYPOINT_NOT_FOUND when the test starts)
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    tauri_build::build()
}
