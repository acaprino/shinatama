//! Deterministic contract exercise. No filesystem, subprocess, authentication or model.
use crate::*;
use std::collections::BTreeSet;

const MAX_FAKE_SESSIONS: usize = 1024;
const MAX_FAKE_RECORDS: usize = 8192;
const MAX_FAKE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct FakeSession {
    pub metadata: SessionMetadata,
    pub records: Vec<TranscriptRecord>,
}

pub struct FakeAdapter {
    descriptor: AdapterDescriptor,
    sessions: Vec<FakeSession>,
}

impl FakeAdapter {
    pub fn new(sessions: Vec<FakeSession>) -> Result<Self, RefusalReason> {
        if sessions.len() > MAX_FAKE_SESSIONS {
            return Err(RefusalReason::LimitExceeded);
        }
        let mut identities = BTreeSet::new();
        let mut record_count = 0_usize;
        let mut byte_count = 0_usize;
        for session in &sessions {
            session.metadata.session.validate()?;
            if !identities.insert((
                session.metadata.session.store_root_id.clone(),
                session.metadata.session.native_id.clone(),
            )) {
                return Err(RefusalReason::InvalidRequest);
            }
            for value in [
                &session.metadata.title,
                &session.metadata.parent_native_id,
                &session.metadata.created_at,
                &session.metadata.updated_at,
                &session.metadata.working_directory,
            ]
            .into_iter()
            .flatten()
            {
                if value.len() > 4096 || value.chars().any(char::is_control) {
                    return Err(RefusalReason::InvalidRequest);
                }
                byte_count += value.len();
            }
            record_count += session.records.len();
            if record_count > MAX_FAKE_RECORDS {
                return Err(RefusalReason::LimitExceeded);
            }
            for record in &session.records {
                if record.role.is_empty()
                    || record.role.len() > 64
                    || record.role.chars().any(char::is_control)
                {
                    return Err(RefusalReason::InvalidRequest);
                }
                byte_count += record.role.len() + record.content.len();
                if byte_count > MAX_FAKE_BYTES {
                    return Err(RefusalReason::LimitExceeded);
                }
            }
            if record_count > MAX_FAKE_RECORDS || byte_count > MAX_FAKE_BYTES {
                return Err(RefusalReason::LimitExceeded);
            }
        }
        let agent = AgentId::new("fake_memory").expect("static agent ID");
        let version = Version::parse("1.0.0").expect("static fixture version");
        let mut capabilities = Capabilities::default();
        for op in [
            Operation::ListSessions,
            Operation::ReadTranscript,
            Operation::ExportInterchange,
        ] {
            let evidence = FixtureEvidence::new(
                agent.clone(),
                version.clone(),
                "fake-memory-v1",
                EvidenceKind::SyntheticFixture,
            )
            .expect("static fixture ID");
            capabilities.qualify(op, &agent, &version, true, evidence)?;
        }
        Ok(Self {
            descriptor: AdapterDescriptor {
                agent,
                display_name: "In-memory contract fixture".to_owned(),
                version,
                architecture: ConnectionArchitecture::InMemory,
                synthetic: true,
                capabilities,
            },
            sessions,
        })
    }

    fn session(&self, cursor: &ReadCursor, op: Operation) -> Result<&FakeSession, AdapterError> {
        cursor
            .session
            .validate()
            .map_err(|reason| AdapterError::new(op, reason))?;
        let session = self
            .sessions
            .iter()
            .find(|entry| {
                entry.metadata.session.store_root_id == cursor.session.store_root_id
                    && entry.metadata.session.native_id == cursor.session.native_id
            })
            .ok_or_else(|| AdapterError::new(op, RefusalReason::NotFound))?;
        if session.metadata.session.store_revision != cursor.session.store_revision {
            return Err(AdapterError::new(op, RefusalReason::StaleCursor));
        }
        if cursor.offset > session.records.len() {
            return Err(AdapterError::new(op, RefusalReason::InvalidRequest));
        }
        Ok(session)
    }

    fn page(
        &self,
        cursor: &ReadCursor,
        limits: PageLimits,
        op: Operation,
    ) -> Result<TranscriptPage, AdapterError> {
        let session = self.session(cursor, op)?;
        let mut records = Vec::new();
        let mut bytes = 0;
        for record in session
            .records
            .iter()
            .skip(cursor.offset)
            .take(limits.records())
        {
            let size = record.content.len() + record.role.len();
            if bytes + size > limits.bytes() {
                break;
            }
            bytes += size;
            records.push(record.clone());
        }
        if records.is_empty() && cursor.offset < session.records.len() {
            return Err(AdapterError::new(op, RefusalReason::LimitExceeded));
        }
        let offset = cursor.offset + records.len();
        Ok(TranscriptPage {
            records,
            next: (offset < session.records.len()).then(|| ReadCursor {
                session: cursor.session.clone(),
                offset,
            }),
        })
    }
}

impl AgentAdapter for FakeAdapter {
    fn descriptor(&self) -> &AdapterDescriptor {
        &self.descriptor
    }

    fn list_sessions(&self, request: &ListRequest) -> Result<SessionPage, AdapterError> {
        request
            .validate()
            .map_err(|reason| AdapterError::new(Operation::ListSessions, reason))?;
        let mut matching = self
            .sessions
            .iter()
            .filter(|s| request.include_children || s.metadata.parent_native_id.is_none())
            .skip(request.offset);
        let sessions: Vec<_> = matching
            .by_ref()
            .take(request.limit)
            .map(|s| s.metadata.clone())
            .collect();
        let next_offset = matching
            .next()
            .is_some()
            .then(|| request.offset.saturating_add(sessions.len()));
        Ok(SessionPage {
            sessions,
            next_offset,
        })
    }

    fn read_transcript(
        &self,
        cursor: &ReadCursor,
        limits: PageLimits,
    ) -> Result<TranscriptPage, AdapterError> {
        self.page(cursor, limits, Operation::ReadTranscript)
    }

    fn export_interchange(
        &self,
        cursor: &ReadCursor,
        limits: PageLimits,
    ) -> Result<ExportPage, AdapterError> {
        let session = self.session(cursor, Operation::ExportInterchange)?;
        Ok(ExportPage {
            schema_version: 1,
            session: session.metadata.clone(),
            transcript: self.page(cursor, limits, Operation::ExportInterchange)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample(root: &str, native_id: &str, values: &[&str]) -> FakeSession {
        FakeSession {
            metadata: SessionMetadata {
                session: SessionRef {
                    store_root_id: root.to_owned(),
                    store_revision: "rev1".to_owned(),
                    native_id: native_id.to_owned(),
                },
                title: None,
                parent_native_id: None,
                created_at: None,
                updated_at: None,
                working_directory: None,
            },
            records: values
                .iter()
                .map(|s| TranscriptRecord {
                    role: "user".to_owned(),
                    content: Content::new((*s).to_owned()).unwrap(),
                })
                .collect(),
        }
    }

    #[test]
    fn metadata_is_body_free_and_missing_fields_stay_unknown() {
        let adapter = FakeAdapter::new(vec![
            sample("box", "same", &["secret"]),
            sample("system", "same", &["other"]),
        ])
        .unwrap();
        let page = adapter
            .list_sessions(&ListRequest {
                offset: 0,
                limit: 1,
                include_children: true,
            })
            .unwrap();
        assert_eq!(page.sessions.len(), 1);
        assert_eq!(page.sessions[0].title, None);
        assert!(
            !serde_json::to_string(&page.sessions[0])
                .unwrap()
                .contains("secret")
        );
        assert_eq!(page.next_offset, Some(1));
    }

    #[test]
    fn bounded_pages_export_losslessly_without_native_import() {
        let source = sample("box", "session", &["primo", "secondo", "terzo"]);
        let cursor = ReadCursor {
            session: source.metadata.session.clone(),
            offset: 0,
        };
        let adapter = FakeAdapter::new(vec![source.clone()]).unwrap();
        let first = adapter
            .export_interchange(&cursor, PageLimits::new(1, 100).unwrap())
            .unwrap();
        let next = adapter
            .export_interchange(
                &first.transcript.next.clone().unwrap(),
                PageLimits::new(2, 100).unwrap(),
            )
            .unwrap();
        let reconstructed: Vec<_> = first
            .transcript
            .records
            .into_iter()
            .chain(next.transcript.records)
            .collect();
        assert_eq!(reconstructed, source.records);
        assert!(next.transcript.next.is_none());
        assert_eq!(
            adapter.unsupported(Operation::ImportNative).reason,
            RefusalReason::NotImplemented
        );
        assert_eq!(
            adapter.unsupported(Operation::StartNew).reason,
            RefusalReason::NotImplemented
        );
    }

    #[test]
    fn stale_cursor_and_oversized_record_cannot_loop_without_progress() {
        let source = sample("box", "session", &["lengthy-record"]);
        let mut cursor = ReadCursor {
            session: source.metadata.session.clone(),
            offset: 0,
        };
        let adapter = FakeAdapter::new(vec![source]).unwrap();
        assert_eq!(
            adapter
                .read_transcript(&cursor, PageLimits::new(1, 5).unwrap())
                .unwrap_err()
                .reason,
            RefusalReason::LimitExceeded
        );
        cursor.session.store_revision = "rev2".to_owned();
        assert_eq!(
            adapter
                .read_transcript(&cursor, PageLimits::new(1, 100).unwrap())
                .unwrap_err()
                .reason,
            RefusalReason::StaleCursor
        );
    }
}
