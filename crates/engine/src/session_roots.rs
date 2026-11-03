//! C22 explicit store binding and pinned Mukade metadata worker protocol.
//! Reader::list materializes a Vec at this pin: ONLY the contained helper calls it.
//! No transcript/export/write/CLI reader, ambient StoreRoots, or credential read.
#[cfg(windows)]
use crate::session_detail::{DetailReply, DetailRequest, TrustedRootSnapshot};
use mukade::{HarnessId, reader::SessionInfo, roots::StoreRoots};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{RootKind, StoreRootRecord},
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const MUKADE_REVISION: &str = "4124b1f880f49a8f958801195324628752e35c4a";
pub const MAX_REQUEST_BYTES: usize = 8192;
pub const MAX_REPLY_BYTES: usize = 256 * 1024;
pub const MAX_ROWS: usize = 50;
pub const WORKER_WALL: Duration = Duration::from_secs(10);
#[cfg(windows)]
pub use execution::{SessionListRead, metadata_worker_budget};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RootSelector {
    pub store_root_id: String,
    pub configuration_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCursor {
    pub offset: usize,
    pub fingerprint: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionReadIssue {
    InvalidRequest,
    RootUnavailable,
    RootChanged,
    UnsafeRoot,
    UnsupportedHarness,
    UnsupportedRootShape,
    SourceChanged,
    MetadataTooLarge,
    ReadUnavailable,
    WorkerUnavailable,
    TimedOut,
    OutputLimit,
    CleanupUnconfirmed,
}
impl std::fmt::Display for SessionReadIssue {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "Session metadata unavailable: {self:?}.")
    }
}
impl std::error::Error for SessionReadIssue {}
type Result<T> = std::result::Result<T, SessionReadIssue>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionRow {
    pub native_id: String,
    pub title: Option<String>,
    pub working_directory: Option<String>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
    pub turns: Option<u64>,
    pub bytes: Option<u64>,
    pub parent_native_id: Option<String>,
    pub archived: Option<bool>,
    pub metadata_truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionListPage {
    pub root: RootSelector,
    pub harness: HarnessId,
    pub fingerprint: String,
    pub total: usize,
    #[serde(deserialize_with = "bounded_rows")]
    pub rows: Vec<SessionRow>,
    pub next: Option<SessionCursor>,
}
fn bounded_rows<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<SessionRow>, D::Error> {
    struct Rows;
    impl<'de> serde::de::Visitor<'de> for Rows {
        type Value = Vec<SessionRow>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most fifty session metadata rows")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = Vec::with_capacity(MAX_ROWS);
            while let Some(row) = sequence.next_element()? {
                if result.len() == MAX_ROWS {
                    return Err(serde::de::Error::custom("session page exceeds row limit"));
                }
                result.push(row);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Rows)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WorkerRequest {
    protocol: u8,
    revision: String,
    root: StoreRootRecord,
    limit: usize,
    cursor: Option<SessionCursor>,
    include_children: bool,
    #[cfg(windows)]
    #[serde(default)]
    detail: Option<DetailRequest>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WorkerReply {
    protocol: u8,
    revision: String,
    page: Option<SessionListPage>,
    #[cfg(windows)]
    detail: Option<DetailReply>,
    issue: Option<SessionReadIssue>,
}
enum SessionPayload {
    List(SessionListPage),
    #[cfg(windows)]
    Detail(DetailReply),
}

/// No public path constructor. SQL identity/revision precede all filesystem reads.
pub struct PreparedSessionList {
    root: StoreRootRecord,
    request: Vec<u8>,
    #[cfg(windows)]
    detail_request: Option<DetailRequest>,
}
impl PreparedSessionList {
    pub fn resolve(
        database: &Database,
        base: &StorageLocation,
        selector: &RootSelector,
        cursor: Option<SessionCursor>,
        limit: usize,
        include_children: bool,
    ) -> Result<Self> {
        if selector.store_root_id.is_empty()
            || selector.store_root_id.len() > 256
            || selector.configuration_revision == 0
            || selector.store_root_id.chars().any(char::is_control)
            || !(1..=MAX_ROWS).contains(&limit)
        {
            return Err(SessionReadIssue::InvalidRequest);
        }
        if cursor
            .as_ref()
            .is_some_and(|cursor| !fingerprint(&cursor.fingerprint))
        {
            return Err(SessionReadIssue::InvalidRequest);
        }
        let root = database
            .store_root(&selector.store_root_id)
            .map_err(|_| SessionReadIssue::ReadUnavailable)?
            .ok_or(SessionReadIssue::RootUnavailable)?;
        if root.config_revision != selector.configuration_revision {
            return Err(SessionReadIssue::RootChanged);
        }
        if !root.available {
            return Err(SessionReadIssue::RootUnavailable);
        }
        let harness = harness(&root.harness)?;
        verify_root(&root, harness)?;
        if root.kind == RootKind::Box {
            // Verification only, never prepare/repair an archive while browsing.
            let private = base
                .resolve_related_directory(Path::new(&root.resolved_path))
                .map_err(|_| SessionReadIssue::UnsafeRoot)?;
            private
                .verify_private_directory()
                .map_err(|_| SessionReadIssue::UnsafeRoot)?;
            let canonical_base =
                fs::canonicalize(base.directory()).map_err(|_| SessionReadIssue::UnsafeRoot)?;
            let canonical_root =
                fs::canonicalize(private.directory()).map_err(|_| SessionReadIssue::UnsafeRoot)?;
            if canonical_root == canonical_base || !canonical_root.starts_with(&canonical_base) {
                return Err(SessionReadIssue::UnsafeRoot);
            }
        }
        let request = serde_json::to_vec(&WorkerRequest {
            protocol: 1,
            revision: MUKADE_REVISION.into(),
            root: root.clone(),
            limit,
            cursor,
            include_children,
            #[cfg(windows)]
            detail: None,
        })
        .map_err(|_| SessionReadIssue::InvalidRequest)?;
        if request.len() > MAX_REQUEST_BYTES {
            return Err(SessionReadIssue::InvalidRequest);
        }
        Ok(Self {
            root,
            request,
            #[cfg(windows)]
            detail_request: None,
        })
    }
    /// Same contained worker and SQL binding as metadata; never reads a transcript
    /// in this host. The helper independently checks the whole-input bound.
    #[cfg(windows)]
    pub fn resolve_detail(
        database: &Database,
        base: &StorageLocation,
        request: DetailRequest,
    ) -> Result<Self> {
        if request.native_id.is_empty()
            || request.native_id.len() > 256
            || !request
                .native_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            || !(1..=MAX_ROWS).contains(&request.limit)
            || request
                .cursor
                .as_ref()
                .is_some_and(|cursor| !fingerprint(&cursor.fingerprint))
        {
            return Err(SessionReadIssue::InvalidRequest);
        }
        let mut prepared = Self::resolve(database, base, &request.root, None, 1, true)?;
        let mut wire: WorkerRequest = serde_json::from_slice(&prepared.request)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        wire.detail = Some(request.clone());
        prepared.request =
            serde_json::to_vec(&wire).map_err(|_| SessionReadIssue::InvalidRequest)?;
        if prepared.request.len() > MAX_REQUEST_BYTES {
            return Err(SessionReadIssue::InvalidRequest);
        }
        prepared.detail_request = Some(request);
        Ok(prepared)
    }
    pub fn selector(&self) -> RootSelector {
        selector(&self.root)
    }
    pub fn request_bytes(&self) -> &[u8] {
        &self.request
    }
    pub fn decode_reply(&self, database: &Database, bytes: &[u8]) -> Result<SessionListPage> {
        match self.decode_payload(database, bytes)? {
            SessionPayload::List(page) => Ok(page),
            #[cfg(windows)]
            SessionPayload::Detail(_) => Err(SessionReadIssue::InvalidRequest),
        }
    }
    fn decode_payload(&self, database: &Database, bytes: &[u8]) -> Result<SessionPayload> {
        if bytes.len() > MAX_REPLY_BYTES {
            return Err(SessionReadIssue::OutputLimit);
        }
        let current = database
            .store_root(&self.root.id)
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        if current.as_ref() != Some(&self.root) {
            return Err(SessionReadIssue::RootChanged);
        }
        let reply: WorkerReply =
            serde_json::from_slice(bytes).map_err(|_| SessionReadIssue::WorkerUnavailable)?;
        if reply.protocol != 1 || reply.revision != MUKADE_REVISION {
            return Err(SessionReadIssue::WorkerUnavailable);
        }
        if let Some(issue) = reply.issue {
            return Err(issue);
        }
        #[cfg(windows)]
        if let Some(request) = &self.detail_request {
            let detail = reply.detail.ok_or(SessionReadIssue::WorkerUnavailable)?;
            if reply.page.is_some()
                || detail.root != self.selector()
                || detail.native_id != request.native_id
                || detail.page.is_some() == detail.issue.is_some()
            {
                return Err(SessionReadIssue::WorkerUnavailable);
            }
            if let Some(page) = &detail.page {
                let offset = request.cursor.as_ref().map_or(0, |cursor| cursor.offset);
                if page.root != self.selector()
                    || page.native_id != request.native_id
                    || page.harness.as_str() != self.root.harness
                    || page.events.len() > request.limit
                    || !fingerprint(&page.fingerprint)
                    || request
                        .cursor
                        .as_ref()
                        .is_some_and(|cursor| cursor.fingerprint != page.fingerprint)
                    || page.events.iter().enumerate().any(|(i, event)| {
                        event.ordinal != offset.saturating_add(i) || event.text.len() > 4096
                    })
                    || offset
                        .checked_add(page.events.len())
                        .is_none_or(|end| end > page.total_events)
                    || page.notices.len() > 6
                    || page.notices.iter().any(|notice| !text(notice, 240))
                    || page.next.as_ref().is_some_and(|next| {
                        next.fingerprint != page.fingerprint
                            || next.offset != offset.saturating_add(page.events.len())
                            || next.offset >= page.total_events
                    })
                {
                    return Err(SessionReadIssue::WorkerUnavailable);
                }
            }
            return Ok(SessionPayload::Detail(detail));
        }
        #[cfg(windows)]
        if reply.detail.is_some() {
            return Err(SessionReadIssue::WorkerUnavailable);
        }
        let page = reply.page.ok_or(SessionReadIssue::WorkerUnavailable)?;
        if page.root != self.selector()
            || page.harness.as_str() != self.root.harness
            || page.rows.len() > MAX_ROWS
            || !fingerprint(&page.fingerprint)
            || page.rows.iter().any(|row| !valid_row(row))
            || page.next.as_ref().is_some_and(|next| {
                next.fingerprint != page.fingerprint || next.offset > page.total
            })
        {
            return Err(SessionReadIssue::WorkerUnavailable);
        }
        Ok(SessionPayload::List(page))
    }
}
fn selector(root: &StoreRootRecord) -> RootSelector {
    RootSelector {
        store_root_id: root.id.clone(),
        configuration_revision: root.config_revision,
    }
}
fn harness(value: &str) -> Result<HarnessId> {
    value
        .parse()
        .map_err(|_| SessionReadIssue::UnsupportedHarness)
}
fn fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn text(value: &str, cap: usize) -> bool {
    value.len() <= cap && !value.chars().any(char::is_control)
}
fn valid_row(row: &SessionRow) -> bool {
    !row.native_id.is_empty()
        && text(&row.native_id, 256)
        && row.title.as_ref().is_none_or(|v| text(v, 512))
        && row.working_directory.as_ref().is_none_or(|v| text(v, 1024))
        && row.updated_at.as_ref().is_none_or(|v| text(v, 64))
        && row.created_at.as_ref().is_none_or(|v| text(v, 64))
        && row.parent_native_id.as_ref().is_none_or(|v| text(v, 256))
}
fn verify_root(root: &StoreRootRecord, harness: HarnessId) -> Result<()> {
    root.validate()
        .map_err(|_| SessionReadIssue::InvalidRequest)?;
    if !root.available || harness.as_str() != root.harness {
        return Err(SessionReadIssue::RootUnavailable);
    }
    let path = Path::new(&root.resolved_path);
    StorageLocation::preflight(path).map_err(|_| SessionReadIssue::UnsafeRoot)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| SessionReadIssue::RootUnavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(SessionReadIssue::UnsafeRoot);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(SessionReadIssue::UnsafeRoot);
        }
    }
    // OpenCode's reviewed StoreRoots contract requires the DB file itself. The
    // current directory-only FS03 registration cannot infer a database filename.
    if mukade::catalog::store_is_file(harness) != metadata.is_file()
        || (!metadata.is_file() && !metadata.is_dir())
    {
        return Err(SessionReadIssue::UnsupportedRootShape);
    }
    let actual = fs::canonicalize(path).map_err(|_| SessionReadIssue::RootUnavailable)?;
    let ordinary = actual
        .to_str()
        .ok_or(SessionReadIssue::UnsafeRoot)?
        .strip_prefix(r"\\?\")
        .unwrap_or(actual.to_str().unwrap());
    if !ordinary.eq_ignore_ascii_case(&root.resolved_path) {
        return Err(SessionReadIssue::UnsafeRoot);
    }
    Ok(())
}
fn roots(root: &StoreRootRecord, harness: HarnessId) -> StoreRoots {
    let mut roots = StoreRoots::default();
    let path = Some(PathBuf::from(&root.resolved_path));
    match harness {
        HarnessId::ClaudeCode => roots.claude_config_dir = path,
        HarnessId::Codex => roots.codex_home = path,
        HarnessId::OpenCode => roots.opencode_db = path,
        HarnessId::Pi => roots.pi_agent_dir = path,
    }
    roots
}

#[cfg(windows)]
pub use namespace::{SessionNamespaceGuard, pin_reader_namespace};
#[cfg(not(windows))]
pub struct SessionNamespaceGuard;
#[cfg(not(windows))]
pub fn pin_reader_namespace(_: &StoreRootRecord) -> Result<SessionNamespaceGuard> {
    Err(SessionReadIssue::ReadUnavailable)
}
#[cfg(not(windows))]
impl SessionNamespaceGuard {
    pub fn verify_current(&self) -> Result<()> {
        Err(SessionReadIssue::ReadUnavailable)
    }
    pub fn file_size(&self, _: &Path) -> Result<u64> {
        Err(SessionReadIssue::ReadUnavailable)
    }
    pub fn total_input_bytes(&self) -> u64 {
        u64::MAX
    }
    pub fn file_count(&self) -> usize {
        usize::MAX
    }
}

#[cfg(windows)]
mod namespace {
    use super::*;
    use std::{
        collections::BTreeMap,
        fs::{File, OpenOptions},
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    };
    const MAX_ENTRIES: usize = 4096;
    const MAX_DIRECTORIES: usize = 1024;
    const MAX_DEPTH: usize = 64;
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Stamp {
        volume: u32,
        id: u64,
        size: u64,
        modified: u64,
        attributes: u32,
        links: u32,
    }
    struct Pin {
        file: File,
        stamp: Stamp,
    }
    /// Holds names of existing objects stable while allowing ordinary appends.
    /// This is NOT isolation from a hostile concurrent writer: Windows directory
    /// sharing cannot forbid new entries. Pre/post checks reject observed changes;
    /// a handle-based upstream reader/private snapshot is needed to close that gap.
    /// Only explicit session subtrees are enumerated; no credential files are read.
    pub struct SessionNamespaceGuard {
        root: StoreRootRecord,
        pins: BTreeMap<PathBuf, Pin>,
    }
    fn inspect(file: &File) -> Result<Stamp> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(SessionReadIssue::ReadUnavailable);
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 && info.nNumberOfLinks != 1)
        {
            return Err(SessionReadIssue::UnsafeRoot);
        }
        let directory = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        Ok(Stamp {
            volume: info.dwVolumeSerialNumber,
            id: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            size: if directory {
                0
            } else {
                (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow)
            },
            // Namespace membership is compared separately. Changes elsewhere in
            // an ancestor (e.g. another Temp directory) must not invalidate data.
            modified: if directory {
                0
            } else {
                (u64::from(info.ftLastWriteTime.dwHighDateTime) << 32)
                    | u64::from(info.ftLastWriteTime.dwLowDateTime)
            },
            attributes: info.dwFileAttributes,
            links: info.nNumberOfLinks,
        })
    }
    fn open(path: &Path) -> Result<Pin> {
        // LIST_DIRECTORY (directories) / READ_DATA (files) participates in Windows
        // share checking; attribute-only handles do not prevent rename. The walk
        // reads metadata only. Writes/appends remain shared; DELETE is denied.
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | 1)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|_| SessionReadIssue::UnsafeRoot)?;
        let stamp = inspect(&file)?;
        Ok(Pin { file, stamp })
    }
    fn insert(
        pins: &mut BTreeMap<PathBuf, Pin>,
        path: &Path,
        directories: &mut usize,
    ) -> Result<bool> {
        if let Some(pin) = pins.get(path) {
            return Ok(pin.stamp.attributes & FILE_ATTRIBUTE_DIRECTORY != 0);
        }
        if pins.len() >= MAX_ENTRIES {
            return Err(SessionReadIssue::MetadataTooLarge);
        }
        let pin = open(path)?;
        let directory = pin.stamp.attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if directory {
            *directories += 1;
            if *directories > MAX_DIRECTORIES {
                return Err(SessionReadIssue::MetadataTooLarge);
            }
        }
        pins.insert(path.to_owned(), pin);
        Ok(directory)
    }
    pub fn pin_reader_namespace(root: &StoreRootRecord) -> Result<SessionNamespaceGuard> {
        SessionNamespaceGuard::capture(root)
    }
    impl SessionNamespaceGuard {
        fn capture(root: &StoreRootRecord) -> Result<Self> {
            let harness = harness(&root.harness)?;
            if harness == HarnessId::OpenCode {
                return Err(SessionReadIssue::UnsupportedRootShape);
            }
            verify_root(root, harness)?;
            let path = Path::new(&root.resolved_path);
            let lineage: Vec<_> = path.ancestors().filter(|path| path.is_absolute()).collect();
            if lineage.len() > MAX_DEPTH {
                return Err(SessionReadIssue::MetadataTooLarge);
            }
            let mut pins = BTreeMap::new();
            let mut directories = 0;
            for ancestor in lineage.into_iter().rev() {
                if !insert(&mut pins, ancestor, &mut directories)? {
                    return Err(SessionReadIssue::UnsafeRoot);
                }
            }
            let subtrees: &[&str] = match harness {
                HarnessId::ClaudeCode => &["projects"],
                HarnessId::Codex => &["sessions", "archived_sessions", "session_index.jsonl"],
                HarnessId::Pi => &["sessions"],
                HarnessId::OpenCode => unreachable!(),
            };
            let mut pending = Vec::new();
            for subtree in subtrees {
                let child = path.join(subtree);
                match fs::symlink_metadata(&child) {
                    Ok(_) => {
                        let directory = insert(&mut pins, &child, &mut directories)?;
                        if directory == (*subtree == "session_index.jsonl") {
                            return Err(SessionReadIssue::UnsupportedRootShape);
                        }
                        if directory {
                            pending.push((child, 0));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(SessionReadIssue::ReadUnavailable),
                }
            }
            while let Some((directory, depth)) = pending.pop() {
                if depth >= MAX_DEPTH {
                    return Err(SessionReadIssue::MetadataTooLarge);
                }
                for child in
                    fs::read_dir(directory).map_err(|_| SessionReadIssue::ReadUnavailable)?
                {
                    let child = child.map_err(|_| SessionReadIssue::ReadUnavailable)?.path();
                    if insert(&mut pins, &child, &mut directories)? {
                        pending.push((child, depth + 1));
                    }
                }
            }
            Ok(Self {
                root: root.clone(),
                pins,
            })
        }
        /// Compare identities, file sizes/mtimes and the bounded namespace again.
        /// Concurrent ordinary mutations refuse the result; no fallback/root repair.
        pub fn verify_current(&self) -> Result<()> {
            for pin in self.pins.values() {
                if inspect(&pin.file).ok() != Some(pin.stamp) {
                    return Err(SessionReadIssue::SourceChanged);
                }
            }
            let current = Self::capture(&self.root).map_err(|_| SessionReadIssue::SourceChanged)?;
            if current.pins.len() != self.pins.len()
                || self.pins.iter().any(|(path, pin)| {
                    current
                        .pins
                        .get(path)
                        .is_none_or(|now| now.stamp != pin.stamp)
                })
            {
                return Err(SessionReadIssue::SourceChanged);
            }
            Ok(())
        }
        /// Exact already-pinned candidate only, never a path selector from IPC.
        /// Call verify_current before and after the actual reader; appends may race.
        pub fn file_size(&self, path: &Path) -> Result<u64> {
            let pin = self
                .pins
                .get(path)
                .filter(|pin| pin.stamp.attributes & FILE_ATTRIBUTE_DIRECTORY == 0)
                .ok_or(SessionReadIssue::UnsafeRoot)?;
            if inspect(&pin.file).ok() != Some(pin.stamp) {
                return Err(SessionReadIssue::SourceChanged);
            }
            Ok(pin.stamp.size)
        }
        /// Conservative metadata-only upper bound, including all pinned files.
        /// Saturation refuses any finite input budget instead of wrapping.
        pub fn total_input_bytes(&self) -> u64 {
            self.pins
                .values()
                .filter(|pin| pin.stamp.attributes & FILE_ATTRIBUTE_DIRECTORY == 0)
                .fold(0_u64, |total, pin| total.saturating_add(pin.stamp.size))
        }
        pub fn file_count(&self) -> usize {
            self.pins
                .values()
                .filter(|pin| pin.stamp.attributes & FILE_ATTRIBUTE_DIRECTORY == 0)
                .count()
        }
    }
}
fn optional(value: Option<String>, cap: usize, truncated: &mut bool) -> Option<String> {
    value.filter(|value| {
        let valid = text(value, cap);
        *truncated |= !valid;
        valid
    })
}
fn row(info: SessionInfo, harness: HarnessId) -> Result<SessionRow> {
    if info.id.is_empty() || !text(&info.id, 256) {
        return Err(SessionReadIssue::MetadataTooLarge);
    }
    let mut truncated = false;
    let result = SessionRow {
        native_id: info.id,
        title: optional(info.title, 512, &mut truncated),
        working_directory: optional(info.cwd, 1024, &mut truncated),
        updated_at: optional(info.updated, 64, &mut truncated),
        created_at: None,
        turns: None,
        bytes: None,
        parent_native_id: optional(info.parent, 256, &mut truncated),
        archived: (harness == HarnessId::OpenCode).then_some(info.archived),
        metadata_truncated: truncated,
    };
    Ok(result)
}
/// Runs ONLY in the isolated helper. No host UI/library method invokes Reader.list.
fn list_in_worker(request: WorkerRequest) -> Result<SessionListPage> {
    if request.protocol != 1
        || request.revision != MUKADE_REVISION
        || !(1..=MAX_ROWS).contains(&request.limit)
    {
        return Err(SessionReadIssue::InvalidRequest);
    }
    let harness = harness(&request.root.harness)?;
    verify_root(&request.root, harness)?;
    let namespace = pin_reader_namespace(&request.root)?;
    namespace.verify_current()?;
    let explicit_roots = roots(&request.root, harness);
    let reader =
        mukade::reader::for_harness(harness).map_err(|_| SessionReadIssue::UnsupportedHarness)?;
    let list = if request.include_children {
        reader.list_all(&explicit_roots)
    } else {
        reader.list(&explicit_roots)
    }
    .map_err(|_| SessionReadIssue::ReadUnavailable)?;
    namespace.verify_current()?;
    // Fingerprint ONLY metadata returned by the reader; never open transcripts to
    // manufacture unknown title/cwd/date values. A changed list invalidates paging.
    let mut hash = Sha256::new();
    hash.update(request.root.id.as_bytes());
    hash.update(request.root.config_revision.to_le_bytes());
    for info in &list {
        for field in [
            Some(info.id.as_str()),
            Some(info.path.as_str()),
            info.title.as_deref(),
            info.cwd.as_deref(),
            info.updated.as_deref(),
            info.parent.as_deref(),
        ] {
            if let Some(value) = field {
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            } else {
                hash.update(u64::MAX.to_le_bytes());
            }
        }
        hash.update([u8::from(info.archived)]);
    }
    let fingerprint = format!("{:x}", hash.finalize());
    let offset = request.cursor.as_ref().map_or(0, |cursor| cursor.offset);
    if request
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.fingerprint != fingerprint || cursor.offset > list.len())
    {
        return Err(SessionReadIssue::SourceChanged);
    }
    let total = list.len();
    let rows = list
        .into_iter()
        .skip(offset)
        .take(request.limit)
        .map(|info| row(info, harness))
        .collect::<Result<Vec<_>>>()?;
    verify_root(&request.root, harness)?;
    let end = offset + rows.len();
    Ok(SessionListPage {
        root: selector(&request.root),
        harness,
        fingerprint: fingerprint.clone(),
        total,
        rows,
        next: (end < total).then_some(SessionCursor {
            offset: end,
            fingerprint,
        }),
    })
}
/// Entry point for shinatama-session-helper only. No secrets or raw errors reach
/// stderr/stdout, including OOM/reader errors. The parent treats process loss as
/// unavailable and retains a reservation whenever native cleanup is unconfirmed.
pub fn helper_main() {
    let result = (|| -> Result<SessionPayload> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::{
                JobObjects::IsProcessInJob, Threading::GetCurrentProcess,
            };
            let mut contained = 0;
            if unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut contained) }
                == 0
                || contained == 0
            {
                return Err(SessionReadIssue::WorkerUnavailable);
            }
        }
        #[cfg(not(windows))]
        return Err(SessionReadIssue::WorkerUnavailable);
        let mut input = Vec::with_capacity(MAX_REQUEST_BYTES);
        std::io::stdin()
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        if input.len() > MAX_REQUEST_BYTES {
            return Err(SessionReadIssue::InvalidRequest);
        }
        let request: WorkerRequest =
            serde_json::from_slice(&input).map_err(|_| SessionReadIssue::InvalidRequest)?;
        if request.protocol != 1 || request.revision != MUKADE_REVISION {
            return Err(SessionReadIssue::InvalidRequest);
        }
        #[cfg(windows)]
        if let Some(detail) = &request.detail {
            let reply = match TrustedRootSnapshot::from_record(&request.root, &detail.root) {
                Ok(snapshot) => crate::session_detail::read_detail(&snapshot, detail),
                Err(issue) => DetailReply::refuse(detail, issue),
            };
            return Ok(SessionPayload::Detail(reply));
        }
        list_in_worker(request).map(SessionPayload::List)
    })();
    let reply = match result {
        Ok(SessionPayload::List(page)) => WorkerReply {
            protocol: 1,
            revision: MUKADE_REVISION.into(),
            page: Some(page),
            #[cfg(windows)]
            detail: None,
            issue: None,
        },
        #[cfg(windows)]
        Ok(SessionPayload::Detail(detail)) => WorkerReply {
            protocol: 1,
            revision: MUKADE_REVISION.into(),
            page: None,
            detail: Some(detail),
            issue: None,
        },
        Err(issue) => WorkerReply {
            protocol: 1,
            revision: MUKADE_REVISION.into(),
            page: None,
            #[cfg(windows)]
            detail: None,
            issue: Some(issue),
        },
    };
    if let Ok(bytes) = serde_json::to_vec(&reply)
        && bytes.len() <= MAX_REPLY_BYTES
    {
        let _ = std::io::stdout().write_all(&bytes);
    }
}

#[cfg(windows)]
mod execution {
    use super::*;
    use crate::resources::{
        admission::{CleanupStatus, PreparedWorkload},
        process::{ContainedProcess, NativeCommand, spawn_contained},
        sampling::WindowsSampler,
    };
    use shinatama_state::resource_reservations::Budget;
    use std::{
        collections::BTreeMap,
        ffi::OsString,
        fs::File,
        os::windows::io::AsRawHandle,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA},
        System::Pipes::PeekNamedPipe,
    };
    const MIB: u64 = 1024 * 1024;
    pub fn metadata_worker_budget() -> Budget {
        Budget {
            memory_bytes: 1024 * MIB,
            processes: 8,
            cpu_basis_points: 1000,
            queue_bytes: MIB,
            queue_items: 4,
        }
    }
    enum Retained {
        Prepared(PreparedWorkload),
        Process(ContainedProcess),
    }
    /// Keep this owner if cleanup is unconfirmed. Neither Drop nor retries issue
    /// another read or release an unrelated ticket/job/process.
    pub struct SessionListRead {
        payload: Option<SessionPayload>,
        pub issue: Option<SessionReadIssue>,
        retained: Option<Retained>,
    }
    impl SessionListRead {
        pub fn page(&self) -> Option<&SessionListPage> {
            match self.payload.as_ref() {
                Some(SessionPayload::List(page)) => Some(page),
                _ => None,
            }
        }
        pub fn detail(&self) -> Option<&DetailReply> {
            match self.payload.as_ref() {
                Some(SessionPayload::Detail(reply)) => Some(reply),
                _ => None,
            }
        }
        pub fn cleanup_retained(&self) -> bool {
            self.retained.is_some()
        }
        pub fn retry_cleanup(&mut self, database: &Database) -> bool {
            let released = match self.retained.as_mut() {
                None => true,
                Some(Retained::Prepared(workload)) => matches!(
                    workload.retry_cleanup(database, Duration::from_secs(1)),
                    Ok(CleanupStatus::Released { .. })
                ),
                Some(Retained::Process(process)) => matches!(
                    process.retry_cleanup(database, Duration::from_secs(1)),
                    Ok(CleanupStatus::Released { .. })
                ),
            };
            if released {
                self.retained = None;
            }
            released
        }
    }
    fn failed(
        database: &Database,
        mut workload: PreparedWorkload,
        issue: SessionReadIssue,
    ) -> SessionListRead {
        let released = matches!(
            workload.finish_cleanup(database, Duration::from_secs(1)),
            Ok(CleanupStatus::Released { .. })
        );
        SessionListRead {
            payload: None,
            issue: Some(issue),
            retained: (!released).then_some(Retained::Prepared(workload)),
        }
    }
    impl PreparedSessionList {
        /// Blocking retained worker only. Runtime must reserve from the SAME
        /// aggregate ledger before entry; no caller-supplied budget is accepted.
        pub fn run_contained(
            &self,
            database: &Database,
            helper_executable: &Path,
            private_working_directory: &StorageLocation,
            workload: PreparedWorkload,
        ) -> SessionListRead {
            let began = Instant::now();
            let fixed = metadata_worker_budget();
            if workload.ticket().budget != fixed
                || !workload.applied_budget().is_some_and(|budget| {
                    budget.job_commit_bytes() == fixed.memory_bytes
                        && budget.process_commit_bytes() == 768 * MIB
                        && budget.max_processes() == 8
                        && budget.cpu_hard_cap_basis_points() == 1000
                })
            {
                return failed(database, workload, SessionReadIssue::WorkerUnavailable);
            }
            let command = (|| {
                private_working_directory
                    .verify_private_directory()
                    .map_err(|_| ())?;
                StorageLocation::preflight(helper_executable).map_err(|_| ())?;
                let system = std::env::var_os("SystemRoot").ok_or(())?;
                let environment = BTreeMap::from([
                    (OsString::from("SystemRoot"), system.clone()),
                    (OsString::from("WINDIR"), system),
                ]);
                NativeCommand::new(
                    helper_executable.to_owned(),
                    Vec::new(),
                    private_working_directory.directory().to_owned(),
                    environment,
                )
                .map_err(|_| ())
            })();
            let Ok(command) = command else {
                return failed(database, workload, SessionReadIssue::WorkerUnavailable);
            };
            let Some(now) = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
            else {
                return failed(database, workload, SessionReadIssue::WorkerUnavailable);
            };
            let mut owned = match spawn_contained(command, workload, database, now, now + 15_000) {
                Ok(owned) => owned,
                Err(failure) => {
                    return failed(
                        database,
                        failure.workload,
                        SessionReadIssue::WorkerUnavailable,
                    );
                }
            };
            let credits = owned.process.queue_credits().acquire(MIB, 4).ok();
            let mut issue = None;
            // NativeCommand allocates a16KiB input pipe; the fixed request is<=8KiB.
            if credits.is_none() || owned.stdin.write_all(&self.request).is_err() {
                issue = Some(SessionReadIssue::WorkerUnavailable);
            }
            drop(owned.stdin);
            let mut output = Vec::with_capacity(MAX_REPLY_BYTES);
            let mut scratch = [0u8; 8192];
            let (mut stdout_done, mut stderr_done) = (false, false);
            let mut total = 0usize;
            let mut exit = None;
            while issue.is_none() {
                match drain(
                    &mut owned.stdout,
                    &mut output,
                    &mut scratch,
                    &mut total,
                    true,
                ) {
                    Ok(done) => stdout_done |= done,
                    Err(error) => {
                        issue = Some(error);
                        break;
                    }
                }
                match drain(
                    &mut owned.stderr,
                    &mut output,
                    &mut scratch,
                    &mut total,
                    false,
                ) {
                    Ok(done) => stderr_done |= done,
                    Err(error) => {
                        issue = Some(error);
                        break;
                    }
                }
                match owned.process.try_wait() {
                    Ok(code) => exit = code,
                    Err(_) => {
                        issue = Some(SessionReadIssue::WorkerUnavailable);
                        break;
                    }
                }
                if exit.is_some() && stdout_done && stderr_done {
                    break;
                }
                if began.elapsed() >= WORKER_WALL {
                    issue = Some(SessionReadIssue::TimedOut);
                    break;
                }
                let _ = owned.process.sample(&WindowsSampler::new());
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut payload = None;
            if issue.is_none() {
                if exit == Some(0) {
                    match self.decode_payload(database, &output) {
                        Ok(value) => payload = Some(value),
                        Err(error) => issue = Some(error),
                    }
                } else {
                    issue = Some(SessionReadIssue::WorkerUnavailable);
                }
            }
            drop(output);
            drop(credits);
            drop(owned.stdout);
            drop(owned.stderr);
            let released = matches!(
                owned.process.finish_cleanup(
                    database,
                    WORKER_WALL
                        .saturating_sub(began.elapsed())
                        .min(Duration::from_secs(1))
                ),
                Ok(CleanupStatus::Released { .. })
            );
            if !released {
                payload = None;
                issue = Some(SessionReadIssue::CleanupUnconfirmed);
            }
            SessionListRead {
                payload,
                issue,
                retained: (!released).then_some(Retained::Process(owned.process)),
            }
        }
    }
    fn drain(
        file: &mut File,
        output: &mut Vec<u8>,
        scratch: &mut [u8],
        total: &mut usize,
        keep: bool,
    ) -> Result<bool> {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                file.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return match std::io::Error::last_os_error()
                .raw_os_error()
                .map(|value| value as u32)
            {
                Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA) => Ok(true),
                _ => Err(SessionReadIssue::WorkerUnavailable),
            };
        }
        if available == 0 {
            return Ok(false);
        }
        let count = (available as usize).min(scratch.len());
        if *total + count > MAX_REPLY_BYTES {
            return Err(SessionReadIssue::OutputLimit);
        }
        let read = file
            .read(&mut scratch[..count])
            .map_err(|_| SessionReadIssue::WorkerUnavailable)?;
        *total += read;
        if keep {
            output.extend_from_slice(&scratch[..read]);
        }
        Ok(read == 0)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use shinatama_state::{
        location::LocationRequest,
        records::{PreparedStoreRoot, RootAccess, RootOrigin, RootScope},
    };
    struct Fixture {
        path: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let base = fs::canonicalize(std::env::temp_dir()).unwrap();
            assert_eq!(self.path.parent(), Some(base.as_path()));
            assert!(
                self.path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("shinatama-session-roots-")
            );
            let _ = fs::remove_dir_all(&self.path);
        }
    }
    #[test]
    fn namespace_pins_allow_appends_but_reject_reparse_hardlinks_and_changed_membership() {
        let fixture = Fixture {
            path: fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("shinatama-session-roots-{}", uuid::Uuid::new_v4())),
        };
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.path.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        location.prepare_private_directory().unwrap();
        let project = location.directory().join("projects/project");
        fs::create_dir_all(&project).unwrap();
        let source = project.join("synthetic.jsonl");
        let original = b"{\"sessionId\":\"synthetic\"}\n";
        fs::write(&source, original).unwrap();
        // A credential-shaped sibling is outside the reader namespace. Even an
        // unsafe alias there must not trigger a scan/read of this unrelated file.
        fs::hard_link(&source, location.directory().join("auth.json")).unwrap();
        let root = StoreRootRecord {
            id: "guard-fixture".into(),
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
        assert!(matches!(
            pin_reader_namespace(&root),
            Err(SessionReadIssue::UnsafeRoot)
        ));
        fs::remove_file(location.directory().join("auth.json")).unwrap();
        fs::write(
            location.directory().join("auth.json"),
            b"synthetic credential sentinel",
        )
        .unwrap();
        let ordinary_source =
            PathBuf::from(&root.resolved_path).join("projects/project/synthetic.jsonl");
        let guard = pin_reader_namespace(&root).unwrap();
        assert_eq!(guard.file_count(), 1);
        assert_eq!(
            guard.file_size(&ordinary_source).unwrap(),
            original.len() as u64
        );
        assert_eq!(guard.total_input_bytes(), original.len() as u64);
        assert!(
            guard
                .file_size(&PathBuf::from(&root.resolved_path).join("auth.json"))
                .is_err()
        );
        guard.verify_current().unwrap();
        assert!(fs::rename(&source, project.join("renamed.jsonl")).is_err());
        let mut writer = fs::OpenOptions::new().append(true).open(&source).unwrap();
        writer.write_all(b"\n").unwrap();
        drop(writer);
        assert!(matches!(
            guard.verify_current(),
            Err(SessionReadIssue::SourceChanged)
        ));
        drop(guard);
        let guard = pin_reader_namespace(&root).unwrap();
        fs::write(project.join("new.jsonl"), b"{}\n").unwrap();
        assert!(matches!(
            guard.verify_current(),
            Err(SessionReadIssue::SourceChanged)
        ));
        drop(guard);
        // The target stays wholly inside this synthetic fixture; no user paths,
        // payload reads or privilege changes are needed for the native refusal.
        let target = location.directory().join("synthetic-outside");
        fs::create_dir(&target).unwrap();
        std::os::windows::fs::symlink_dir(&target, project.join("linked-directory")).unwrap();
        assert!(matches!(
            pin_reader_namespace(&root),
            Err(SessionReadIssue::UnsafeRoot)
        ));
        assert_eq!(
            fs::read(location.directory().join("auth.json")).unwrap(),
            b"synthetic credential sentinel"
        );
    }
    #[test]
    fn real_pin_metadata_stays_scoped_and_unavailable_roots_never_fall_back() {
        let fixture = Fixture {
            path: fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("shinatama-session-roots-{}", uuid::Uuid::new_v4())),
        };
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: std::env::temp_dir(),
            home_override: Some(fixture.path.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        let database = Database::open(&location, None).unwrap();
        let mut bindings = Vec::new();
        let mut sources = Vec::new();
        for revision in 1..=2 {
            let home = location
                .resolve_related_directory(
                    &location.directory().join(format!("archive-{revision}")),
                )
                .unwrap();
            home.prepare_private_directory().unwrap();
            let project = home.directory().join("projects").join("fixture-project");
            fs::create_dir_all(&project).unwrap();
            let source = project.join("same-native-id.jsonl");
            let bytes = br#"{"sessionId":"same-native-id","cwd":"C:\\synthetic-project"}
"#;
            fs::write(&source, bytes).unwrap();
            let prepared = PreparedStoreRoot::resolve(StoreRootRecord {
                id: format!("fixture-root-{revision}"),
                harness: "claude_code".into(),
                kind: RootKind::Box,
                scope: RootScope::Primary,
                resolved_path: home
                    .directory()
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .into(),
                origin: RootOrigin::Configured,
                config_revision: revision,
                access: RootAccess::ReadWrite,
                available: true,
            })
            .unwrap();
            let record = if revision == 1 {
                database.register_store_root(&prepared).unwrap()
            } else {
                database
                    .register_store_root_revision("fixture-root-1", 1, &prepared)
                    .unwrap()
            };
            let read = PreparedSessionList::resolve(
                &database,
                &location,
                &selector(&record),
                None,
                50,
                true,
            )
            .unwrap();
            // Tiny structure-only fixture against the REAL pinned reader. Product
            // calls must use the contained helper rather than the test seam.
            let page =
                list_in_worker(serde_json::from_slice(read.request_bytes()).unwrap()).unwrap();
            assert_eq!(page.rows.len(), 1);
            assert_eq!(page.rows[0].native_id, "same-native-id");
            assert_eq!(page.rows[0].title, None);
            assert_eq!(page.rows[0].created_at, None);
            assert_eq!(page.rows[0].turns, None);
            assert_eq!(page.rows[0].bytes, None);
            assert_eq!(page.rows[0].archived, None);
            let reply = serde_json::to_vec(&WorkerReply {
                protocol: 1,
                revision: MUKADE_REVISION.into(),
                page: Some(page),
                detail: None,
                issue: None,
            })
            .unwrap();
            bindings.push(read.decode_reply(&database, &reply).unwrap().root);
            sources.push((source, bytes.to_vec()));
        }
        assert_ne!(bindings[0], bindings[1]);
        database
            .mark_store_root_unavailable("fixture-root-1", 1)
            .unwrap();
        assert!(matches!(
            PreparedSessionList::resolve(&database, &location, &bindings[0], None, 50, true),
            Err(SessionReadIssue::RootUnavailable)
        ));
        let mut stale = bindings[1].clone();
        stale.configuration_revision = 1;
        assert!(matches!(
            PreparedSessionList::resolve(&database, &location, &stale, None, 50, true),
            Err(SessionReadIssue::RootChanged)
        ));
        for (path, bytes) in sources {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}
