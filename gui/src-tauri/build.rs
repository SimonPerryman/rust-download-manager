fn main() {
    let mut attributes = tauri_build::Attributes::new();
    // The Windows app manifest (Common Controls v6) goes in through the
    // linker instead of tauri's resource file, so the test binaries get it
    // too: without it the updater tests' mock app fails to load
    // (TaskDialogIndirect needs Common Controls v6).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
        let manifest = std::path::Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("tauri build script failed");
}
