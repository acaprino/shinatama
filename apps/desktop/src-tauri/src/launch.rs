//! Typed read-only launch review. The engine owns all admission decisions.
use shinatama_engine::{
    Engine,
    launch::{LaunchIntent, LaunchSummary},
};
use tauri::{State, WebviewWindow};

#[tauri::command(async)]
pub fn launch_preflight(
    window: WebviewWindow,
    engine: State<'_, Engine>,
    request: LaunchIntent,
) -> Result<LaunchSummary, &'static str> {
    if window.label() != "main" {
        return Err("Launch review is unavailable in this window.");
    }
    // IPC supplies selectors only. Trusted inputs/recipes cannot be supplied here.
    // Start and Resume both pass through this exact same service operation.
    Ok(engine.preflight_launch(&request, None).summary().clone())
}
