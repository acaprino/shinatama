//! Private, local shared state. Location validation precedes every persistent file.

pub mod cancellation;
pub mod database;
pub mod location;
pub mod maintenance;
mod migrations;
pub mod projects;
pub mod records;
mod schema;
pub mod store_roots;
