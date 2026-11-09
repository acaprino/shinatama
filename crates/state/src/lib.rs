//! Private, local shared state. Location validation precedes every persistent file.

pub mod cancellation;
pub mod database;
pub mod global_workspace;
pub mod hops;
pub mod location;
pub mod maintenance;
mod migrations;
pub mod native_session_cleanup;
pub mod projects;
pub mod records;
pub mod resource_reservations;
pub mod search_index;
mod schema;
pub mod store_roots;
pub mod threads;
