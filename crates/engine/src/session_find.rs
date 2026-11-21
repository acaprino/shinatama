//! Explicit literal search in one verified canonical session, inside the reader
//! worker. No persisted index, RAW, reasoning or attachment payload is searched. The text is
//! every word of the session, read one entry at a time through Mukade's streamed read, so no
//! session is too large to search (ADR-0053 v2).
use crate::{
    session_detail::{
        DetailIssue, DetailRequest, StreamError, StreamStop, TrustedRootSnapshot, open,
        visible_event_count,
    },
    session_roots::RootSelector,
};
use mukade::{Entry, Kind, model::ResultContent};
use serde::{Deserialize, Serialize};

pub const MAX_QUERY_BYTES: usize = 128;
pub const MAX_FIND_HITS: usize = 50;
pub const MAX_SNIPPET_BYTES: usize = 512;
pub const MAX_FIND_REPLY_BYTES: usize = 256 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindCursor {
    pub offset: usize,
    pub fingerprint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindRequest {
    pub detail: DetailRequest,
    pub query: String,
    pub cursor: Option<FindCursor>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum FindIssue {
    InvalidQuery,
    FingerprintRequired,
    OutputLimit,
    Detail(DetailIssue),
}
/// No Debug: snippets are conversation content, never diagnostics.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindHit {
    pub entry_ordinal: usize,
    pub event_ordinal: usize,
    pub match_ordinal: usize,
    pub snippet: String,
    pub truncated: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindPage {
    pub root: RootSelector,
    pub native_id: String,
    pub source_fingerprint: String,
    pub query: String,
    pub total_hits: usize,
    pub hits: Vec<FindHit>,
    pub next: Option<FindCursor>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindReply {
    pub root: RootSelector,
    pub native_id: String,
    pub page: Option<FindPage>,
    pub issue: Option<FindIssue>,
}
impl FindReply {
    pub fn refuse(request: &FindRequest, issue: FindIssue) -> Self {
        Self {
            root: request.detail.root.clone(),
            native_id: if request.detail.native_id.len() <= 256 {
                request.detail.native_id.clone()
            } else {
                String::new()
            },
            page: None,
            issue: Some(issue),
        }
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// Required before source access, including for the first result page.
pub fn validate_request(request: &FindRequest) -> Result<(), FindIssue> {
    if request.query.trim().is_empty()
        || request.query.len() > MAX_QUERY_BYTES
        || request.query.chars().any(char::is_control)
    {
        return Err(FindIssue::InvalidQuery);
    }
    let source = request
        .detail
        .cursor
        .as_ref()
        .ok_or(FindIssue::FingerprintRequired)?;
    if source.offset != 0 || !digest(&source.fingerprint) {
        return Err(FindIssue::Detail(DetailIssue::InvalidRequest));
    }
    if request.cursor.as_ref().is_some_and(|cursor| {
        !digest(&cursor.fingerprint) || cursor.fingerprint != source.fingerprint
    }) {
        return Err(FindIssue::Detail(DetailIssue::SourceChanged));
    }
    Ok(())
}
pub fn read_find(snapshot: &TrustedRootSnapshot, request: &FindRequest) -> FindReply {
    read_find_progress(snapshot, request, &mut |_, _| {})
}
/// [`read_find`] telling `progress(entries read, hits so far)` after every entry; the caller
/// decides how often that is worth reporting.
pub fn read_find_progress(
    snapshot: &TrustedRootSnapshot,
    request: &FindRequest,
    progress: &mut dyn FnMut(usize, usize),
) -> FindReply {
    let result = (|| {
        validate_request(request)?;
        let opened = open(snapshot, &request.detail).map_err(FindIssue::Detail)?;
        let mut search = Search {
            folded_query: request.query.to_lowercase(),
            offset: request.cursor.as_ref().map_or(0, |cursor| cursor.offset),
            total: 0,
            hits: Vec::with_capacity(MAX_FIND_HITS),
        };
        let mut event_ordinal = 0usize;
        let mut entry_ordinal = 0usize;
        let mut failure = None;
        let streamed = opened.streamed(snapshot, &request.detail, &mut |entry| {
            match scan(&mut search, entry_ordinal, event_ordinal, &entry) {
                Ok(events) => {
                    entry_ordinal += 1;
                    event_ordinal += events;
                    progress(entry_ordinal, search.total);
                    Ok(())
                }
                Err(issue) => {
                    failure = Some(issue);
                    Err(StreamStop)
                }
            }
        });
        let fingerprint = match streamed {
            Ok((_, fingerprint)) => fingerprint,
            Err(StreamError::Detail(issue)) => return Err(FindIssue::Detail(issue)),
            Err(StreamError::Stopped) => return Err(failure.unwrap_or(FindIssue::OutputLimit)),
        };
        if search.offset > search.total {
            return Err(FindIssue::Detail(DetailIssue::SourceChanged));
        }
        let end = search.offset + search.hits.len();
        let reply = FindReply {
            root: request.detail.root.clone(),
            native_id: request.detail.native_id.clone(),
            page: Some(FindPage {
                root: request.detail.root.clone(),
                native_id: request.detail.native_id.clone(),
                source_fingerprint: fingerprint.clone(),
                query: request.query.clone(),
                total_hits: search.total,
                hits: search.hits,
                next: (end < search.total).then_some(FindCursor {
                    offset: end,
                    fingerprint,
                }),
            }),
            issue: None,
        };
        if !serde_json::to_vec(&reply)
            .is_ok_and(|bytes| bytes.len() <= MAX_FIND_REPLY_BYTES)
        {
            return Err(FindIssue::OutputLimit);
        }
        Ok(reply)
    })();
    result.unwrap_or_else(|issue| FindReply::refuse(request, issue))
}
/// Searches one entry's visible text in the detail's order and separators; returns how many
/// detail events the entry holds, so the next entry's events are numbered after them.
fn scan(
    search: &mut Search,
    entry_ordinal: usize,
    event_ordinal: usize,
    entry: &Entry,
) -> Result<usize, FindIssue> {
    match &entry.kind {
        Kind::User { text, .. } => search.text(text, entry_ordinal, event_ordinal)?,
        Kind::Step { text, .. } => {
            // Same canonical text order/separator as the detail event.
            search.text(&text.join("\n"), entry_ordinal, event_ordinal)?;
        }
        Kind::Results { results } => {
            for (index, result) in results.iter().enumerate() {
                if let ResultContent::Text(text) = &result.content {
                    search.text(text, entry_ordinal, event_ordinal + index)?;
                }
            }
        }
        Kind::Compaction { summary, .. } => {
            search.text(summary, entry_ordinal, event_ordinal)?;
        }
        Kind::Meta { .. } => {}
    }
    event_ordinal
        .checked_add(visible_event_count(entry))
        .map(|next| next - event_ordinal)
        .ok_or(FindIssue::OutputLimit)
}
struct Search {
    folded_query: String,
    offset: usize,
    total: usize,
    hits: Vec<FindHit>,
}
impl Search {
    fn text(
        &mut self,
        text: &str,
        entry_ordinal: usize,
        event_ordinal: usize,
    ) -> Result<(), FindIssue> {
        // One bounded source field at a time. No per-character offset vector:
        // map folded offsets monotonically back to the original UTF-8 text.
        let folded = text.to_lowercase();
        let mut original = text.char_indices();
        let mut folded_end = 0usize;
        let mut original_start = 0usize;
        for (start, _) in folded.match_indices(&self.folded_query) {
            let ordinal = self.total;
            self.total = self.total.checked_add(1).ok_or(FindIssue::OutputLimit)?;
            if ordinal < self.offset || self.hits.len() == MAX_FIND_HITS {
                continue;
            }
            while folded_end <= start {
                let Some((at, character)) = original.next() else {
                    return Err(FindIssue::OutputLimit);
                };
                original_start = at;
                folded_end += character.to_lowercase().map(char::len_utf8).sum::<usize>();
            }
            let (snippet, truncated) = snippet(text, original_start);
            self.hits.push(FindHit {
                entry_ordinal,
                event_ordinal,
                match_ordinal: ordinal,
                snippet,
                truncated,
            });
        }
        Ok(())
    }
}
fn snippet(text: &str, at: usize) -> (String, bool) {
    let mut start = at.saturating_sub(64);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (start + MAX_SNIPPET_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[start..end].to_owned(), start != 0 || end != text.len())
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
            if self.0.parent() == Some(fs::canonicalize(std::env::temp_dir()).unwrap().as_path())
                && self.0.file_name().unwrap().to_string_lossy().starts_with("shinatama-session-find-")
            {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
    #[test]
    fn real_reader_search_pages_canonical_text_excludes_private_payloads_and_pins_source() {
        let fixture = Fixture(fs::canonicalize(std::env::temp_dir()).unwrap()
            .join(format!("shinatama-session-find-{}", uuid::Uuid::new_v4())));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(), home_override: Some(fixture.0.clone()),
            project_roots: Vec::new(), synchronized_roots: Vec::new(),
        }).unwrap();
        location.prepare_private_directory().unwrap();
        let project = location.directory().join("projects/synthetic");
        fs::create_dir_all(&project).unwrap();
        let path = project.join("find-fixture.jsonl");
        let mut rows = Vec::new();
        for index in 0..53 {
            rows.push(serde_json::json!({"type":"user","uuid":format!("u{index}"),
                "parentUuid":(index>0).then(||format!("u{}",index-1)),"sessionId":"find-fixture",
                "synthetic_secret":"private-needle","message":{"role":"user","content":format!("🦀 Alpha row {index}")}}));
        }
        rows.push(serde_json::json!({"type":"assistant","uuid":"a","parentUuid":"u52","sessionId":"find-fixture",
            "message":{"id":"a-msg","role":"assistant","model":"synthetic","stop_reason":"end_turn","content":[
                {"type":"thinking","thinking":"private-needle","signature":"private-needle"},
                {"type":"text","text":"visible answer"}]}}));
        let bytes = rows.iter().map(|row|serde_json::to_string(row).unwrap()).collect::<Vec<_>>().join("\n")+"\n";
        fs::write(&path,&bytes).unwrap();
        let record = StoreRootRecord { id:"find-root".into(), harness:"claude_code".into(),
            kind:RootKind::Box, scope:RootScope::Primary,
            resolved_path:location.directory().to_string_lossy().trim_start_matches(r"\\?\").into(),
            origin:RootOrigin::Configured,config_revision:1,access:RootAccess::ReadWrite,available:true };
        let root=RootSelector {store_root_id:record.id.clone(),configuration_revision:1};
        let snapshot=TrustedRootSnapshot::from_record(&record,&root).unwrap();
        let mut detail=DetailRequest {root,native_id:"find-fixture".into(),cursor:None,limit:50, overview_only: false};
        let first=read_detail(&snapshot,&detail); assert!(first.issue.is_none());
        detail.cursor=Some(DetailCursor{offset:0,fingerprint:first.page.unwrap().fingerprint});
        let mut request=FindRequest {detail,query:"aLPHa".into(),cursor:None};
        let found=read_find(&snapshot,&request); assert!(found.issue.is_none());
        let page=found.page.unwrap(); assert_eq!(page.total_hits,53); assert_eq!(page.hits.len(),50);
        assert!(page.hits.iter().all(|hit| hit.snippet.contains("Alpha") && hit.snippet.len()<=512));
        let target=&page.hits[20];
        let mut jump=request.detail.clone(); jump.cursor.as_mut().unwrap().offset=target.event_ordinal;
        let jumped=read_detail(&snapshot,&jump).page.unwrap();
        assert_eq!(jumped.events[0].entry_ordinal,target.entry_ordinal);
        assert!(jumped.events[0].text.contains("Alpha"));
        request.cursor = page.next;
        let tail = read_find(&snapshot, &request).page.unwrap();
        assert_eq!(tail.hits.len(), 3);
        assert!(tail.next.is_none());
        assert_eq!(tail.hits[0].match_ordinal, 50);
        request.cursor = None;
        request.query = "private-needle".into();
        let private = read_find(&snapshot, &request).page.unwrap();
        assert_eq!(private.total_hits, 0);
        request.query = "🦀".repeat(33);
        assert_eq!(
            read_find(&snapshot, &request).issue,
            Some(FindIssue::InvalidQuery)
        );
        request.query = "Alpha".into();
        let mut changed = bytes.clone();
        changed.push('\n');
        fs::write(&path, changed).unwrap();
        assert_eq!(
            read_find(&snapshot, &request).issue,
            Some(FindIssue::Detail(DetailIssue::SourceChanged))
        );
        request.detail.cursor = None;
        assert_eq!(
            read_find(&snapshot, &request).issue,
            Some(FindIssue::FingerprintRequired)
        );
    }

    #[test]
    fn search_reads_every_word_of_a_session_past_16_mib() {
        use crate::session_detail::{DetailEventKind, large_fixture};
        let large = large_fixture::build();
        let mut detail = large_fixture::request(&large);
        let view = read_detail(&large.snapshot, &detail);
        assert!(view.issue.is_none(), "{:?}", view.issue);
        let page = view.page.unwrap();
        // The view shows 4 KiB of the result: the marker, 12 MiB in, is not in it.
        assert!(page.events.iter().all(|event| !event.text.contains(large_fixture::DEEP_MARKER)));
        detail.cursor = Some(DetailCursor { offset: 0, fingerprint: page.fingerprint });
        let mut request = FindRequest { detail, query: large_fixture::DEEP_MARKER.to_lowercase(), cursor: None };
        let found = read_find(&large.snapshot, &request);
        assert!(found.issue.is_none(), "{:?}", found.issue);
        let hits = found.page.unwrap();
        assert_eq!(hits.total_hits, 1);
        assert!(hits.hits[0].snippet.contains(large_fixture::DEEP_MARKER));
        // The hit's ordinals open the page that holds the result it was found in.
        let mut jump = request.detail.clone();
        jump.cursor.as_mut().unwrap().offset = hits.hits[0].event_ordinal;
        let opened = read_detail(&large.snapshot, &jump).page.unwrap();
        assert!(matches!(opened.events[0].kind, DetailEventKind::ToolResult));
        // A change of the file after the page was read is still refused.
        request.query = "done reading".into();
        let mut grown = std::fs::read(&large.path).unwrap();
        grown.push(10);
        std::fs::write(&large.path, grown).unwrap();
        assert_eq!(read_find(&large.snapshot, &request).issue, Some(FindIssue::Detail(DetailIssue::SourceChanged)));
    }
}
