//! The updater's test seam, compiled into debug builds only: environment
//! variables that point it at a local fixture instead of GitHub.
//!
//! - `RDM_UPDATE_ENDPOINT`: the manifest URL to read;
//! - `RDM_UPDATE_PUBKEY`: the public key its signatures are checked against;
//! - `RDM_UPDATE_BUNDLE`: the installer kind to act as (`nsis`, `msi`, `app`,
//!   `appimage`, `deb`), since a debug build isn't installed from one;
//! - `RDM_UPDATE_DRY_INSTALL=1`: stop where the installer would run;
//! - `RDM_UPDATE_FAIL_INSTALL=1`: make the installer step fail, as it does
//!   on this platform (on Windows after the before-exit hook).

use tauri::utils::config::BundleType;

use super::Seam;

pub(super) fn from_env() -> Seam {
    let var = |name| std::env::var(name).ok().filter(|v: &String| !v.is_empty());
    Seam {
        endpoint: var("RDM_UPDATE_ENDPOINT").and_then(|u| u.parse().ok()),
        pubkey: var("RDM_UPDATE_PUBKEY"),
        bundle: var("RDM_UPDATE_BUNDLE").and_then(|b| match b.as_str() {
            "nsis" => Some(BundleType::Nsis),
            "msi" => Some(BundleType::Msi),
            "app" => Some(BundleType::App),
            "appimage" => Some(BundleType::AppImage),
            "deb" => Some(BundleType::Deb),
            _ => None,
        }),
        dry_install: var("RDM_UPDATE_DRY_INSTALL").is_some_and(|v| v == "1"),
        fail_install: var("RDM_UPDATE_FAIL_INSTALL").is_some_and(|v| v == "1"),
    }
}
