//! FS03 objects, applied only through the verified-backup v2-to-v3 migration.
//! Existing project/setting objects remain unchanged. No future feature/queue/index
//! tables are created merely to reserve their names.

pub const STORE_ROOT: &str = "CREATE TABLE store_root (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(id AS BLOB)) BETWEEN 1 AND 256),
    harness TEXT NOT NULL CHECK(length(harness) BETWEEN 1 AND 64),
    kind TEXT NOT NULL CHECK(kind IN ('box','system')),
    scope TEXT NOT NULL CHECK(scope IN ('primary','additional')),
    resolved_path TEXT NOT NULL CHECK(length(CAST(resolved_path AS BLOB)) BETWEEN 1 AND 4096),
    origin TEXT NOT NULL CHECK(origin IN ('configured','env','catalog','scan')),
    config_revision INTEGER NOT NULL CHECK(config_revision BETWEEN 1 AND 9007199254740991),
    access TEXT NOT NULL CHECK(access IN ('read_only','read_write')),
    available INTEGER NOT NULL CHECK(available IN (0,1)),
    previous_root_id TEXT UNIQUE REFERENCES store_root(id),
    CHECK(scope!='additional' OR (kind='system' AND access='read_only'))
) STRICT";

pub const STORE_ROOT_BY_HARNESS: &str = "CREATE INDEX store_root_by_harness ON store_root(harness,kind,available,config_revision DESC,id)";
pub const STORE_ROOT_PRIMARY_SCOPE: &str = "CREATE UNIQUE INDEX store_root_primary_scope ON store_root(harness,kind,config_revision) WHERE scope='primary'";

pub const STORE_ROOT_IMMUTABLE: &str =
    "CREATE TRIGGER store_root_immutable BEFORE UPDATE ON store_root
WHEN NEW.id!=OLD.id OR NEW.harness!=OLD.harness OR NEW.kind!=OLD.kind
 OR NEW.scope!=OLD.scope OR NEW.previous_root_id IS NOT OLD.previous_root_id
 OR NEW.resolved_path!=OLD.resolved_path OR NEW.origin!=OLD.origin
 OR NEW.config_revision!=OLD.config_revision OR NEW.access!=OLD.access
 OR (OLD.available=0 AND NEW.available=1)
BEGIN SELECT RAISE(ABORT,'store_root_binding_immutable'); END";

pub const WORKSPACE: &str = "CREATE TABLE workspace (
    project_id TEXT NOT NULL REFERENCES project(id),
    workspace_id TEXT NOT NULL CHECK(length(CAST(workspace_id AS BLOB)) BETWEEN 1 AND 256),
    revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 9007199254740991),
    schema_version INTEGER NOT NULL CHECK(schema_version>0),
    layout_json TEXT NOT NULL CHECK(json_valid(layout_json) AND length(CAST(layout_json AS BLOB))<=262144),
    owner_window_id TEXT,
    owner_pid INTEGER,
    owner_start_time TEXT,
    updated_at INTEGER NOT NULL CHECK(updated_at>=0),
    PRIMARY KEY(project_id,workspace_id),
    CHECK((owner_window_id IS NULL AND owner_pid IS NULL AND owner_start_time IS NULL)
       OR (owner_window_id IS NOT NULL AND length(CAST(owner_window_id AS BLOB)) BETWEEN 1 AND 256
       AND owner_pid IS NOT NULL AND owner_pid BETWEEN 1 AND 4294967295 AND owner_start_time IS NOT NULL
       AND length(CAST(owner_start_time AS BLOB)) BETWEEN 1 AND 256))
) STRICT";

pub const WORKSPACE_OWNER_GUARD: &str =
    "CREATE TRIGGER workspace_owner_guard BEFORE UPDATE ON workspace
WHEN OLD.owner_window_id IS NOT NULL AND NEW.owner_window_id IS NOT NULL
 AND (NEW.owner_window_id!=OLD.owner_window_id OR NEW.owner_pid!=OLD.owner_pid
 OR NEW.owner_start_time!=OLD.owner_start_time)
BEGIN SELECT RAISE(ABORT,'workspace_owned_elsewhere'); END";

/// Exact bounded definitions shared by schema validation and ordered migrations.
/// Merely importing this module does not apply them.
pub(crate) const OBJECTS: &[(&str, &str)] = &[
    ("store_root", STORE_ROOT),
    ("store_root_by_harness", STORE_ROOT_BY_HARNESS),
    ("store_root_primary_scope", STORE_ROOT_PRIMARY_SCOPE),
    ("store_root_immutable", STORE_ROOT_IMMUTABLE),
    ("workspace", WORKSPACE),
    ("workspace_owner_guard", WORKSPACE_OWNER_GUARD),
];

pub(crate) const MIGRATION_V3: &[&str] = &[
    STORE_ROOT,
    STORE_ROOT_BY_HARNESS,
    STORE_ROOT_PRIMARY_SCOPE,
    STORE_ROOT_IMMUTABLE,
    WORKSPACE,
    WORKSPACE_OWNER_GUARD,
];

/// Classification covers only names explicitly defined by the functional spec.
/// This is not an activation/cleanup allowlist. Any new operational feature must
/// first qualify its own owner, retention and count/byte budgets (FRD-0014/R34).
pub const AUTHORITATIVE_NAMES: &[&str] = &[
    "project",
    "store_root",
    "workspace",
    "thread",
    "segment",
    "activation",
    "hop",
    "hop_event",
    "snapshot",
    "message",
    "message_event",
    "roundtable",
    "roundtable_participant",
    "setting",
    "learning_memory",
];
pub const OPERATIONAL_NAMES: &[&str] = &[
    "instance",
    "event",
    "summary_log",
    "session_index",
    "session_text",
];
pub const DERIVED_DEFERRED_NAMES: &[&str] = &["thread_task"];
// The companion's authoritative record has no exact table name in the accepted
// text. Do not invent a named table or treat its cards as disposable summary_log.
