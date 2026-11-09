//! The bundle identity installers ship with. The identifier names the app on
//! each OS (bundle id, install keys, webview data folder), so changing it after
//! a release strands installed copies.

use serde_json::Value;

fn tauri_conf() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read tauri.conf.json"))
        .expect("parse tauri.conf.json")
}

#[test]
fn identifier_is_the_released_one() {
    assert_eq!(tauri_conf()["identifier"], "io.github.simonperryman.rdm");
}

#[test]
fn bundle_metadata_is_filled() {
    let bundle = &tauri_conf()["bundle"];
    for key in ["publisher", "copyright", "license", "homepage", "category"] {
        let value = bundle[key].as_str().unwrap_or_default();
        assert!(!value.is_empty(), "bundle.{key} is not set");
    }
    assert_eq!(bundle["license"], "MIT OR Apache-2.0");
}

#[test]
fn version_comes_from_the_crate() {
    // Without a `version` key Tauri uses the GUI crate's, which the workspace
    // sets for the CLI too.
    assert!(tauri_conf().get("version").is_none());
}
