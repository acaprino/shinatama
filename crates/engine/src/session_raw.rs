//! Confirmed single-entry RAW, invoked ONLY in the contained reader worker.
//! Native payloads have no Debug implementation, log path or default projection.
use crate::{
    session_detail::{DetailIssue, DetailRequest, TrustedRootSnapshot, read_clipped_verified},
    session_roots::RootSelector,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

pub const MAX_RAW_BYTES: usize = 64 * 1024;
pub const MAX_RAW_REPLY_BYTES: usize = 256 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawRequest {
    pub detail: DetailRequest,
    pub entry_ordinal: usize,
    pub confirmed_raw: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum RawIssue {
    ConfirmationRequired,
    FingerprintRequired,
    EntryNotFound,
    RawUnavailable,
    OutputLimit,
    Detail(DetailIssue),
}
/// Raw JSON exists only here after confirmation. No Debug/log formatting.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawReply {
    pub root: RootSelector,
    pub native_id: String,
    pub entry_ordinal: usize,
    pub source_fingerprint: Option<String>,
    #[serde(deserialize_with = "bounded_raw")]
    pub raw_json: Option<String>,
    pub issue: Option<RawIssue>,
}
impl RawReply {
    pub fn refuse(request: &RawRequest, issue: RawIssue) -> Self {
        refuse(request, issue)
    }
}
fn bounded_raw<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(deserializer)?;
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_RAW_BYTES)
    {
        return Err(serde::de::Error::custom(
            "RAW output exceeds its fixed bound",
        ));
    }
    Ok(value)
}
struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("RAW output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|count| *count <= MAX_RAW_REPLY_BYTES)
            .ok_or_else(|| io::Error::other("RAW reply limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn refuse(request: &RawRequest, issue: RawIssue) -> RawReply {
    RawReply {
        root: request.detail.root.clone(),
        native_id: if request.detail.native_id.len() <= 256 {
            request.detail.native_id.clone()
        } else {
            String::new()
        },
        entry_ordinal: request.entry_ordinal,
        source_fingerprint: None,
        raw_json: None,
        issue: Some(issue),
    }
}
/// Confirmation is checked BEFORE any source read. Require the fingerprint of
/// the visible detail page, so entry indices cannot silently shift on an append.
pub fn read_raw(snapshot: &TrustedRootSnapshot, request: &RawRequest) -> RawReply {
    if !request.confirmed_raw {
        return refuse(request, RawIssue::ConfirmationRequired);
    }
    if request.detail.cursor.is_none() {
        return refuse(request, RawIssue::FingerprintRequired);
    }
    if request
        .detail
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.offset != 0)
    {
        return refuse(request, RawIssue::Detail(DetailIssue::InvalidRequest));
    }
    let result = (|| {
        // One entry's records, shown whole up to MAX_RAW_BYTES: Mukade clips every string at that
        // length, so an entry whose RAW would not fit still does not (a clipped string alone fills
        // the bound), and one that fits is the full record. The read holds the session's
        // structure, never its text, whatever its size.
        let (session, source_fingerprint) =
            read_clipped_verified(snapshot, &request.detail, MAX_RAW_BYTES)
                .map_err(RawIssue::Detail)?;
        let entry = session
            .entries
            .get(request.entry_ordinal)
            .ok_or(RawIssue::EntryNotFound)?;
        if entry.raw.is_empty() {
            return Err(RawIssue::RawUnavailable);
        }
        // Do not clone Value or create the full serialized record first. The
        // serializer writes directly into this fixed allocation and refuses excess.
        let mut writer = BoundedWriter {
            bytes: Vec::with_capacity(MAX_RAW_BYTES),
            limit: MAX_RAW_BYTES,
        };
        serde_json::to_writer_pretty(&mut writer, &entry.raw).map_err(|_| RawIssue::OutputLimit)?;
        let raw_json = String::from_utf8(writer.bytes).map_err(|_| RawIssue::OutputLimit)?;
        let reply = RawReply {
            root: request.detail.root.clone(),
            native_id: request.detail.native_id.clone(),
            entry_ordinal: request.entry_ordinal,
            source_fingerprint: Some(source_fingerprint),
            raw_json: Some(raw_json),
            issue: None,
        };
        serde_json::to_writer(Counter(0), &reply).map_err(|_| RawIssue::OutputLimit)?;
        Ok(reply)
    })();
    result.unwrap_or_else(|issue| refuse(request, issue))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::session_detail::{DetailCursor, read_detail};
    use shinatama_state::{
        location::{LocationRequest, StorageLocation},
        records::{RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
    };
    use std::{fs, path::PathBuf};
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
            if self.0.parent() == Some(parent.as_path())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("shinatama-session-raw-")
            {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
    #[test]
    fn real_reader_raw_is_confirmed_single_entry_and_opaque_absent_or_oversized_remain_distinct() {
        let fixture = Fixture(
            fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("shinatama-session-raw-{}", uuid::Uuid::new_v4())),
        );
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.0.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        location.prepare_private_directory().unwrap();
        let project = location.directory().join("projects/synthetic");
        fs::create_dir_all(&project).unwrap();
        let path = project.join("raw-fixture.jsonl");
        let rows = [
            serde_json::json!({"type":"user","uuid":"u","parentUuid":null,"sessionId":"raw-fixture","message":{"role":"user","content":"hello"}}),
            serde_json::json!({"type":"assistant","uuid":"a","parentUuid":"u","sessionId":"raw-fixture","message":{"id":"a-message","role":"assistant","model":"synthetic","stop_reason":"end_turn","content":[{"type":"thinking","thinking":"synthetic-hidden","signature":"synthetic-opaque"},{"type":"text","text":"answer"}]}}),
            serde_json::json!({"type":"user","uuid":"r","parentUuid":"a","sessionId":"raw-fixture","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"synthetic-tool","content":"ok"},{"type":"text","text":"follow-up"}]}}),
            serde_json::json!({"type":"user","uuid":"b","parentUuid":"r","sessionId":"raw-fixture","message":{"role":"user","content":"small"},"synthetic_large_payload":"x".repeat(MAX_RAW_BYTES)}),
        ];
        let bytes = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&path, &bytes).unwrap();
        let record = StoreRootRecord {
            id: "raw-root".into(),
            harness: "claude_code".into(),
            kind: RootKind::Box,
            scope: RootScope::Primary,
            resolved_path: location
                .directory()
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .into(),
            origin: RootOrigin::Configured,
            config_revision: 1,
            access: RootAccess::ReadWrite,
            available: true,
        };
        let root = RootSelector {
            store_root_id: record.id.clone(),
            configuration_revision: 1,
        };
        let snapshot = TrustedRootSnapshot::from_record(&record, &root).unwrap();
        let mut detail = DetailRequest {
            root,
            native_id: "raw-fixture".into(),
            cursor: None,
            limit: 50,
            overview_only: false,
        };
        let normal = read_detail(&snapshot, &detail);
        assert!(normal.issue.is_none());
        let normal_wire = serde_json::to_string(&normal).unwrap();
        assert!(
            !normal_wire.contains("synthetic-hidden") && !normal_wire.contains("synthetic-opaque")
        );
        detail.cursor = Some(DetailCursor {
            offset: 0,
            fingerprint: normal.page.unwrap().fingerprint,
        });
        let (session, _) = read_clipped_verified(&snapshot, &detail, MAX_RAW_BYTES).unwrap();
        let opaque = session
            .entries
            .iter()
            .position(|entry| {
                entry
                    .raw
                    .iter()
                    .any(|row| row.pointer("/message/content/0/signature").is_some())
            })
            .unwrap();
        let absent = session
            .entries
            .iter()
            .position(|entry| entry.raw.is_empty())
            .unwrap();
        let large = session
            .entries
            .iter()
            .position(|entry| {
                entry
                    .raw
                    .iter()
                    .any(|row| row.get("synthetic_large_payload").is_some())
            })
            .unwrap();
        let mut request = RawRequest {
            detail,
            entry_ordinal: opaque,
            confirmed_raw: false,
        };
        assert_eq!(
            read_raw(&snapshot, &request).issue,
            Some(RawIssue::ConfirmationRequired)
        );
        request.confirmed_raw = true;
        let raw = read_raw(&snapshot, &request);
        assert!(raw.issue.is_none());
        assert!(raw.raw_json.as_ref().unwrap().contains("synthetic-opaque"));
        assert!(
            !raw.raw_json
                .as_ref()
                .unwrap()
                .contains("synthetic_large_payload")
        );
        request.entry_ordinal = absent;
        assert_eq!(
            read_raw(&snapshot, &request).issue,
            Some(RawIssue::RawUnavailable)
        );
        request.entry_ordinal = large;
        let refused = read_raw(&snapshot, &request);
        assert_eq!(refused.issue, Some(RawIssue::OutputLimit));
        assert!(refused.raw_json.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
        // A session past 16 MiB is read like any other: the entry asked for comes back whole, and
        // an entry whose record does not fit the RAW bound is still refused for that and only that.
        let mut grown = bytes.clone();
        grown.push_str(
            &serde_json::to_string(&serde_json::json!({"type":"user","uuid":"big","parentUuid":"b",
                "sessionId":"raw-fixture","message":{"role":"user","content":"y".repeat(17 * 1024 * 1024)}}))
            .unwrap(),
        );
        grown.push('\n');
        fs::write(&path, &grown).unwrap();
        let mut again = request.detail.clone();
        again.cursor = None;
        let fresh = read_detail(&snapshot, &again);
        assert!(fresh.issue.is_none(), "{:?}", fresh.issue);
        again.cursor = Some(DetailCursor {
            offset: 0,
            fingerprint: fresh.page.unwrap().fingerprint,
        });
        let (grown_session, _) = read_clipped_verified(&snapshot, &again, MAX_RAW_BYTES).unwrap();
        request.detail = again;
        request.entry_ordinal = opaque;
        let past_bound = read_raw(&snapshot, &request);
        assert!(past_bound.issue.is_none(), "{:?}", past_bound.issue);
        assert!(
            past_bound
                .raw_json
                .as_ref()
                .unwrap()
                .contains("synthetic-opaque")
        );
        request.entry_ordinal = grown_session.entries.len() - 1;
        assert_eq!(
            read_raw(&snapshot, &request).issue,
            Some(RawIssue::OutputLimit)
        );
        request.confirmed_raw = false;
        assert_eq!(
            read_raw(&snapshot, &request).issue,
            Some(RawIssue::ConfirmationRequired)
        );
    }
}
