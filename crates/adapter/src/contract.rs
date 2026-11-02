use crate::{Capabilities, Operation, RefusalReason, Version};
use serde::Serialize;
use std::fmt;

pub const MAX_PAGE_RECORDS: usize = 256;
pub const MAX_PAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(value: &str) -> Result<Self, &'static str> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err("invalid_agent_id");
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramPlacement {
    System,
    Own,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomePlacement {
    Box,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Placement {
    program: ProgramPlacement,
    home: HomePlacement,
}

impl Placement {
    pub fn new(program: ProgramPlacement, home: HomePlacement) -> Result<Self, RefusalReason> {
        if program == ProgramPlacement::Own && home == HomePlacement::System {
            return Err(RefusalReason::InvalidRequest);
        }
        Ok(Self { program, home })
    }
    pub fn program(self) -> ProgramPlacement {
        self.program
    }
    pub fn home(self) -> HomePlacement {
        self.home
    }
}

/// Selection records a design decision; it does not itself qualify a capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectionArchitecture {
    NativePty,
    BackendAttach { protocol: String },
    StructuredBackend { protocol: String },
    InMemory,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdapterDescriptor {
    pub agent: AgentId,
    pub display_name: String,
    pub version: Version,
    pub architecture: ConnectionArchitecture,
    pub synthetic: bool,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionRef {
    pub store_root_id: String,
    pub store_revision: String,
    pub native_id: String,
}

impl SessionRef {
    pub fn validate(&self) -> Result<(), RefusalReason> {
        if [&self.store_root_id, &self.store_revision, &self.native_id]
            .into_iter()
            .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
        {
            return Err(RefusalReason::InvalidRequest);
        }
        Ok(())
    }
}

/// Option preserves unknown metadata; listing has no transcript body field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionMetadata {
    pub session: SessionRef,
    pub title: Option<String>,
    pub parent_native_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub working_directory: Option<String>,
}

/// In-memory content deliberately omits Debug/Serialize to avoid accidental logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Content(String);

impl Content {
    pub fn new(value: String) -> Result<Self, RefusalReason> {
        if value.len() > MAX_PAGE_BYTES {
            return Err(RefusalReason::LimitExceeded);
        }
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Content([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptRecord {
    pub role: String,
    pub content: Content,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReadCursor {
    pub session: SessionRef,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageLimits {
    records: usize,
    bytes: usize,
}

impl PageLimits {
    pub fn new(records: usize, bytes: usize) -> Result<Self, RefusalReason> {
        if records == 0 || records > MAX_PAGE_RECORDS || bytes == 0 || bytes > MAX_PAGE_BYTES {
            return Err(RefusalReason::LimitExceeded);
        }
        Ok(Self { records, bytes })
    }
    pub fn records(self) -> usize {
        self.records
    }
    pub fn bytes(self) -> usize {
        self.bytes
    }
}

#[derive(Clone, Debug)]
pub struct TranscriptPage {
    pub records: Vec<TranscriptRecord>,
    pub next: Option<ReadCursor>,
}

#[derive(Clone, Debug)]
pub struct SessionPage {
    pub sessions: Vec<SessionMetadata>,
    pub next_offset: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ListRequest {
    pub offset: usize,
    pub limit: usize,
    pub include_children: bool,
}

impl ListRequest {
    pub fn validate(&self) -> Result<(), RefusalReason> {
        if self.limit == 0 || self.limit > MAX_PAGE_RECORDS {
            return Err(RefusalReason::LimitExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ExportPage {
    pub schema_version: u32,
    pub session: SessionMetadata,
    pub transcript: TranscriptPage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AdapterError {
    pub operation: Operation,
    pub reason: RefusalReason,
}

impl AdapterError {
    pub fn new(operation: Operation, reason: RefusalReason) -> Self {
        Self { operation, reason }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {:?}", self.operation, self.reason)
    }
}
impl std::error::Error for AdapterError {}

/// No shell, arbitrary command, generic SQL, authentication or model bridge.
/// Native start/resume plans are introduced only with C09R/C11 containment.
pub trait AgentAdapter: Send + Sync {
    fn descriptor(&self) -> &AdapterDescriptor;
    fn list_sessions(&self, _request: &ListRequest) -> Result<SessionPage, AdapterError> {
        Err(self.unsupported(Operation::ListSessions))
    }
    fn read_transcript(
        &self,
        _cursor: &ReadCursor,
        _limits: PageLimits,
    ) -> Result<TranscriptPage, AdapterError> {
        Err(self.unsupported(Operation::ReadTranscript))
    }
    fn export_interchange(
        &self,
        _cursor: &ReadCursor,
        _limits: PageLimits,
    ) -> Result<ExportPage, AdapterError> {
        Err(self.unsupported(Operation::ExportInterchange))
    }
    fn unsupported(&self, operation: Operation) -> AdapterError {
        AdapterError::new(
            operation,
            match self.descriptor().capabilities.require(operation) {
                Ok(()) => RefusalReason::NotImplemented,
                Err(reason) => reason,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_refuses_own_binary_with_system_home() {
        assert_eq!(
            Placement::new(ProgramPlacement::Own, HomePlacement::System),
            Err(RefusalReason::InvalidRequest)
        );
        assert!(Placement::new(ProgramPlacement::System, HomePlacement::Box).is_ok());
        assert!(Placement::new(ProgramPlacement::Own, HomePlacement::Box).is_ok());
    }

    #[test]
    fn content_debug_is_redacted_and_limits_cannot_be_unbounded() {
        let content = Content::new("private transcript".to_owned()).unwrap();
        assert!(!format!("{content:?}").contains("private"));
        assert!(PageLimits::new(0, 1).is_err());
        assert!(PageLimits::new(1, MAX_PAGE_BYTES + 1).is_err());
    }
}
