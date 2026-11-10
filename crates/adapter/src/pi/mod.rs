//! Pi (`@earendil-works/pi-coding-agent`) recipes as pure data. Install layout,
//! argv, per-instance resource sets, auth-status contract and declared
//! capabilities. No file read, process, credential, shell string or network
//! access lives here: the engine pins, contains and runs what these plan.
pub mod auth;
pub mod discovery;
