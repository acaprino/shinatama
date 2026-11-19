//! Private, local shared state. Location validation precedes every persistent file.

pub mod cancellation;
pub mod database;
pub mod forget;
pub mod global_workspace;
pub mod hops;
pub mod location;
pub mod maintenance;
pub mod messages;
mod migrations;
pub mod native_session_cleanup;
pub mod privacy;
pub mod projects;
pub mod records;
pub mod resource_reservations;
pub mod search_index;
mod schema;
pub mod store_roots;
#[doc(hidden)]
pub mod test_scratch;
pub mod threads;
pub mod worksets;
