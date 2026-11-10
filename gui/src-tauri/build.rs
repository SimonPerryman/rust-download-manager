use std::process::Command;

fn main() {
    git_commit();
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

/// Expose the commit being built as `RDM_GIT_SHA` (for the log's startup
/// line), when built from a git checkout with git on PATH.
fn git_commit() {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    let Some(sha) = git(&["rev-parse", "--short=10", "HEAD"]) else {
        return;
    };
    println!("cargo:rustc-env=RDM_GIT_SHA={sha}");
    // Rebuild when HEAD moves: a checkout changes HEAD, a commit its branch
    // (a loose ref, or packed-refs once packed).
    let mut watched = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in watched {
        let path = git(&["rev-parse", "--path-format=absolute", "--git-path", &name]);
        if let Some(path) = path.filter(|p| std::path::Path::new(p).exists()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
