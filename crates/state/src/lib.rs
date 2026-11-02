//! Private, local shared state. Location validation precedes every persistent file.

pub mod database;
pub mod cancellation;
pub mod location;
pub mod maintenance;
mod migrations;
mod schema;
