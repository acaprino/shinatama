//! The arguments of the WebView2 browser, which every window of the app shares.
//!
//! WebView2 refuses a second webview on the same data folder with other arguments, so the attention widget gets exactly
//! the ones the main window has: the shell's (`imago_shell::browser_args`) on top of the app's own. The app's own is
//! none in a release build; a debug build, the one the browser smokes start, forwards `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
//! after wry's own arguments, because WebView2 ignores that variable when the application passes arguments through its
//! API, which wry always does (runtime 157 on Verify Windows: the desktop smokes asked for a remote-debugging port there
//! and the browser never got it).
use tauri::utils::config::WindowConfig;

/// The arguments wry passes when it is given none (wry 0.57 webview2/mod.rs: the mini menu and SmartScreen off, autoplay
/// allowed), so giving any keeps them.
const WRY_BROWSER_ARGUMENTS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

fn test_browser_arguments() -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    browser_arguments(
        std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")
            .ok()
            .as_deref(),
    )
}

fn browser_arguments(requested: Option<&str>) -> Option<String> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty())?;
    Some(format!("{WRY_BROWSER_ARGUMENTS} {requested}"))
}

/// The window's config with the arguments the test run asked for, when it asked for any.
pub fn with_test_arguments(mut config: WindowConfig) -> WindowConfig {
    if let Some(arguments) = test_browser_arguments() {
        config.additional_browser_args = Some(arguments);
    }
    config
}

/// What every window of the app passes to WebView2, the main window's arguments included.
pub fn shared(main: &WindowConfig) -> String {
    imago_shell::browser_args(&with_test_arguments(main.clone()))
}

#[cfg(test)]
mod tests {
    use super::browser_arguments;

    #[test]
    fn test_arguments_follow_wry_own_and_nothing_is_added_without_them() {
        assert_eq!(browser_arguments(None), None);
        assert_eq!(browser_arguments(Some("  ")), None);
        assert_eq!(
            browser_arguments(Some(
                " --remote-debugging-port=9222 --remote-debugging-address=127.0.0.1 "
            ))
            .as_deref(),
            Some(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
                 --autoplay-policy=no-user-gesture-required \
                 --remote-debugging-port=9222 --remote-debugging-address=127.0.0.1"
            )
        );
    }
}
