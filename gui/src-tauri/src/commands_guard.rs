//! Keeps a panicking command from taking the app down. Unguarded, a panic in
//! a synchronous command unwinds into the webview's IPC callback, which can't
//! unwind, so the process aborts; one in an async command kills its task and
//! the webview's `invoke` never settles. Guarded, either rejects the `invoke`
//! with [`internal_error`], which the frontend shows like any failed command;
//! the panic hook has already logged the details.

use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures_util::FutureExt;
use tauri::ipc::Invoke;
use tauri::Runtime;

/// What a webview `invoke` rejects with when `command` panicked.
pub(crate) fn internal_error(command: &str) -> String {
    format!("rdm hit an internal error running {command}, a bug. Details are in the log.")
}

/// Wrap the app's invoke handler so a panic while running a synchronous
/// command (or starting an async one) rejects that `invoke` instead.
pub(crate) fn contain_panics<R, H>(handler: H) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static
where
    R: Runtime,
    H: Fn(Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke| {
        let resolver = invoke.resolver.clone();
        let command = invoke.message.command().to_owned();
        std::panic::catch_unwind(AssertUnwindSafe(|| handler(invoke))).unwrap_or_else(|_| {
            resolver.reject(internal_error(&command));
            true
        })
    }
}

/// Run an async command's body so a panic in it rejects the `invoke`, which
/// [`contain_panics`] can't see: async commands run on their own task.
pub(crate) async fn contained<T>(
    command: &str,
    body: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    AssertUnwindSafe(body)
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(internal_error(command)))
}

/// Test-only (the `fault-injection` feature): a synchronous command that
/// panics, as a bug in a command handler would.
#[cfg(feature = "fault-injection")]
#[tauri::command]
pub(crate) fn test_panic() {
    panic!("injected fault: command panicked");
}

/// Test-only: [`test_panic`] for an async command.
#[cfg(feature = "fault-injection")]
#[tauri::command]
pub(crate) async fn test_panic_async() -> Result<(), String> {
    contained("test_panic_async", async {
        panic!("injected fault: async command panicked");
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_async_body_becomes_an_error() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let failed: Result<(), String> =
            runtime.block_on(contained("pick_folder", async { panic!("boom") }));
        assert_eq!(failed, Err(internal_error("pick_folder")));
        let fine = runtime.block_on(contained("pick_folder", async { Ok(7) }));
        assert_eq!(fine, Ok(7));
    }
}
