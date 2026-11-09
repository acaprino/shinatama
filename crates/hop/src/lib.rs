//! Seed hand-off between agents (FRD-0005 R19-R33, ADR-0019).
//!
//! Input is the canonical, already redacted export projection (STX items); `reasoning` has no
//! representation there. [`history`] removes noise and earlier seeds and applies the seed
//! redaction rules, [`seed`] renders the budgeted seed, [`files`] writes the private hand-off
//! files, [`channel`] builds the bootstrap for each delivery channel and [`preview`] freezes the
//! exact bytes the user confirms.
pub mod channel;
pub mod coordinator;
pub mod files;
pub mod git;
pub mod history;
pub mod preview;
pub mod recovery;
pub mod seed;

pub use history::{HANDOFF_MARKER, History, normalize};
pub use seed::{Budget, HeaderFacts, Seed, SeedRefusal};
