fn main() {
    // Tests qui créent une app Tauri (diagnostic de update.rs) : le manifeste des contrôles
    // communs de Windows, comme l'exe (sinon STATUS_ENTRYPOINT_NOT_FOUND au lancement du test)
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    tauri_build::build()
}
