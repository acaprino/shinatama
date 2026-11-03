//! Private, local shared state. Location validation precedes every persistent file.

pub mod cancellation;
pub mod database;
pub mod location;
pub mod maintenance;
mod migrations;
pub mod projects;
pub mod records;
pub mod resource_reservations;
mod schema;
pub mod store_roots;
pub mod threads;
