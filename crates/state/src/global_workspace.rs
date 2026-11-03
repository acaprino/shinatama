//! One project-independent presentation document. Legacy workspace rows remain intact.
use crate::{database::Database, records::{RecordsError, Result, WorkspaceOwner, MAX_SAFE_REVISION, validate_layout_json, recovery::{DeadOwnerIdentityProof, OwnerObservation, probe_owner_identity}}};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

const KEY: &str = "ui.workspace.main.v1";
const MAX_BYTES: usize = 300 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlobalWorkspaceRecord {
    pub workspace_id: String, pub revision: u64, pub schema_version: u32,
    pub layout_json: String, pub owner: Option<WorkspaceOwner>, pub updated_at: i64,
}
impl GlobalWorkspaceRecord {
    pub fn validate(&self) -> Result<()> {
        if self.workspace_id != "main" || self.revision == 0 || self.revision > MAX_SAFE_REVISION || self.updated_at < 0 {
            return Err(RecordsError::Invalid("workspace globale"));
        }
        validate_layout_json(self.schema_version, &self.layout_json)?;
        if let Some(owner) = &self.owner { owner.validate()?; }
        Ok(())
    }
}
pub struct DeadGlobalWorkspaceOwnerProof { revision: u64, death: DeadOwnerIdentityProof }
pub struct GlobalOwnerProbe { pub observation: OwnerObservation, pub proof: Option<DeadGlobalWorkspaceOwnerProof> }
/// Native observation outside SQL; no invented project identity and no PID-only recovery.
pub fn probe_global_workspace_owner(record: &GlobalWorkspaceRecord) -> Result<GlobalOwnerProbe> {
    record.validate()?;
    if record.revision >= MAX_SAFE_REVISION { return Err(RecordsError::Invalid("revisione recupero")); }
    let owner = record.owner.as_ref().ok_or(RecordsError::OwnedElsewhere)?;
    let probe = probe_owner_identity(owner)?;
    Ok(GlobalOwnerProbe { observation: probe.observation, proof: probe.proof.map(|death| DeadGlobalWorkspaceOwnerProof { revision: record.revision, death }) })
}
fn read(connection: &Connection) -> Result<Option<GlobalWorkspaceRecord>> {
    let value: Option<Option<String>> = connection.query_row(
        "SELECT CASE WHEN octet_length(value)<=?1 THEN value END FROM setting WHERE key=?2", params![MAX_BYTES as i64, KEY], |row| row.get(0),
    ).optional()?;
    let Some(value) = value else { return Ok(None); };
    let value = value.ok_or(RecordsError::UnsupportedLayout)?;
    let record: GlobalWorkspaceRecord = serde_json::from_str(&value).map_err(|_| RecordsError::UnsupportedLayout)?;
    record.validate()?;
    Ok(Some(record))
}
fn write(connection: &Connection, record: &GlobalWorkspaceRecord) -> Result<()> {
    record.validate()?;
    let value = serde_json::to_string(record).map_err(|_| RecordsError::Invalid("workspace globale"))?;
    if value.len() > MAX_BYTES { return Err(RecordsError::Invalid("dimensione workspace globale")); }
    connection.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![KEY, value])?;
    Ok(())
}
impl Database {
    pub fn global_workspace_record(&self) -> Result<Option<GlobalWorkspaceRecord>> {
        let connection = self.lock()?;
        read(&connection)
    }
    pub fn save_global_workspace(&self, mut record: GlobalWorkspaceRecord, expected_revision: u64, owner: &WorkspaceOwner) -> Result<GlobalWorkspaceRecord> {
        owner.validate()?;
        if expected_revision >= MAX_SAFE_REVISION { return Err(RecordsError::Invalid("revisione")); }
        record.revision = expected_revision + 1; record.owner = Some(owner.clone()); record.validate()?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        match read(&tx)? {
            Some(current) => {
                if current.revision != expected_revision { return Err(RecordsError::Conflict); }
                if current.owner.as_ref().is_some_and(|saved| saved != owner) { return Err(RecordsError::OwnedElsewhere); }
            }
            None if expected_revision != 0 => return Err(RecordsError::Conflict),
            None => {}
        }
        write(&tx, &record)?; tx.commit()?; Ok(record)
    }
    pub fn release_global_workspace(&self, expected_revision: u64, owner: &WorkspaceOwner) -> Result<()> {
        owner.validate()?; self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let mut current = read(&tx)?.ok_or(RecordsError::NotFound)?;
        if current.revision != expected_revision { return Err(RecordsError::Conflict); }
        if current.owner.as_ref() != Some(owner) { return Err(RecordsError::OwnedElsewhere); }
        if current.revision >= MAX_SAFE_REVISION { return Err(RecordsError::Invalid("revisione")); }
        current.revision += 1; current.owner = None;
        write(&tx, &current)?; tx.commit()?; Ok(())
    }
    pub fn recover_global_workspace(&self, proof: &DeadGlobalWorkspaceOwnerProof, replacement: Option<&WorkspaceOwner>) -> Result<GlobalWorkspaceRecord> {
        if let Some(owner) = replacement { owner.validate()?; }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let mut current = read(&tx)?.ok_or(RecordsError::NotFound)?;
        if current.revision != proof.revision || !current.owner.as_ref().is_some_and(|owner| proof.death.matches(owner)) { return Err(RecordsError::Conflict); }
        if current.revision >= MAX_SAFE_REVISION { return Err(RecordsError::Invalid("revisione")); }
        current.revision += 1; current.owner = replacement.cloned();
        write(&tx, &current)?; tx.commit()?; Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{location::{LocationRequest, StorageLocation}, records::WorkspaceRecord};
    #[test]
    fn independent_connections_preserve_project_layout_and_enforce_global_cas_owner_and_unknown_state() {
        let root = std::env::temp_dir().join(format!("shinatama-global-workspace-{}", uuid::Uuid::new_v4()));
        let location = StorageLocation::resolve(LocationRequest { local_data_directory: root.clone(), home_override: Some(root.join("private")), project_roots: vec![], synchronized_roots: vec![] }).unwrap();
        let first = Database::open(&location, None).unwrap(); let second = Database::open(&location, None).unwrap();
        let project = root.join("project"); std::fs::create_dir(&project).unwrap(); let project = first.select(&project).unwrap();
        let owner = WorkspaceOwner { window_id: "main-A".into(), pid: 7, start_time: "win-101".into() };
        let other = WorkspaceOwner { window_id: "main-B".into(), pid: 7, start_time: "win-102".into() };
        let legacy = first.save_workspace(WorkspaceRecord { project_id: project.id.clone(), workspace_id: "main".into(), revision: 1, schema_version: 1, layout_json: "{\"legacy\":true}".into(), owner: None, updated_at: 12 }, 0, &owner).unwrap();
        let record = GlobalWorkspaceRecord { workspace_id: "main".into(), revision: 1, schema_version: 1, layout_json: "{\"views\":[]}".into(), owner: None, updated_at: 13 };
        assert!(first.global_workspace_record().unwrap().is_none());
        let saved = first.save_global_workspace(record.clone(), 0, &owner).unwrap();
        assert_eq!(second.global_workspace_record().unwrap().unwrap().layout_json, saved.layout_json);
        assert!(matches!(second.save_global_workspace(record.clone(), 0, &owner), Err(RecordsError::Conflict)));
        assert!(matches!(second.save_global_workspace(record.clone(), 1, &other), Err(RecordsError::OwnedElsewhere)));
        assert!(matches!(second.release_global_workspace(1, &other), Err(RecordsError::OwnedElsewhere)));
        first.release_global_workspace(1, &owner).unwrap();
        assert!(matches!(first.save_global_workspace(record.clone(), 1, &owner), Err(RecordsError::Conflict)));
        let claimed = second.save_global_workspace(record.clone(), 2, &other).unwrap(); assert_eq!(claimed.revision, 3);
        assert_eq!(serde_json::to_value(first.workspace_record(&project.id, "main").unwrap().unwrap()).unwrap(), serde_json::to_value(legacy).unwrap());
        let raw = rusqlite::Connection::open(location.database_path()).unwrap();
        let future = "{\"workspaceId\":\"main\",\"revision\":4,\"schemaVersion\":99,\"layoutJson\":\"{}\",\"owner\":null,\"updatedAt\":14}";
        raw.execute("UPDATE setting SET value=?1 WHERE key=?2", params![future, KEY]).unwrap();
        assert!(matches!(first.global_workspace_record(), Err(RecordsError::UnsupportedLayout)));
        assert!(second.save_global_workspace(record, 4, &other).is_err());
        assert_eq!(raw.query_row::<String, _, _>("SELECT value FROM setting WHERE key=?1", [KEY], |row| row.get(0)).unwrap(), future);
        let saved_json = serde_json::to_string(&claimed).unwrap();
        raw.execute("UPDATE setting SET value=?1 WHERE key=?2", params![saved_json, KEY]).unwrap();
        raw.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(first.save_global_workspace(claimed, 3, &other), Err(RecordsError::Database(crate::database::DatabaseError::NewerSchema { found: 999, .. }))));
        assert_eq!(raw.query_row::<String, _, _>("SELECT value FROM setting WHERE key=?1", [KEY], |row| row.get(0)).unwrap(), saved_json);
        // This UUID-owned synthetic fixture is retained; no user data cleanup.
    }
}
