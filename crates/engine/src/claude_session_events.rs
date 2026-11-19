//! Compatibility names for the Claude transport. The worker, limits and global
//! receiver lease live only in the shared harness transport.
pub use crate::harness_session_events::{
    EVENT_QUEUE_CAPACITY, MAX_BODY_BYTES, MAX_CWD_BYTES, MAX_HEADER_BYTES, REQUEST_TIMEOUT,
    ReadError, ReceiverStatus, SessionEvent, SessionEventKind,
    SessionEventReceiver as ClaudeSessionEventReceiver, StartError,
};
