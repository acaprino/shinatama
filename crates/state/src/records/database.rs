use super::*;

const ROOT_BY_ID: &str = "SELECT CASE WHEN octet_length(id)<=256 THEN id END,CASE WHEN octet_length(harness)<=64 THEN harness END,CASE WHEN octet_length(kind)<=6 THEN kind END,CASE WHEN octet_length(scope)<=10 THEN scope END,CASE WHEN octet_length(resolved_path)<=4096 THEN resolved_path END,CASE WHEN octet_length(origin)<=10 THEN origin END,config_revision,CASE WHEN octet_length(access)<=10 THEN access END,CASE WHEN available IN (0,1) THEN available END FROM store_root WHERE id=?1";
const ROOT_PAGE: &str = "SELECT CASE WHEN octet_length(id)<=256 THEN id END,CASE WHEN octet_length(harness)<=64 THEN harness END,CASE WHEN octet_length(kind)<=6 THEN kind END,CASE WHEN octet_length(scope)<=10 THEN scope END,CASE WHEN octet_length(resolved_path)<=4096 THEN resolved_path END,CASE WHEN octet_length(origin)<=10 THEN origin END,config_revision,CASE WHEN octet_length(access)<=10 THEN access END,CASE WHEN available IN (0,1) THEN available END FROM store_root WHERE id>?1 ORDER BY id LIMIT ?2";

fn sql_revision(value: u64) -> Result<i64> {
    if value > MAX_SAFE_REVISION {
        return Err(RecordsError::Invalid("revisione"));
    }
    i64::try_from(value).map_err(|_| RecordsError::Invalid("revisione"))
}

fn read_revision(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(column)?;
    let revision = u64::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery)?;
    if revision == 0 || revision > MAX_SAFE_REVISION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(revision)
}

fn root_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoreRootRecord> {
    let kind = match row.get::<_, String>(2)?.as_str() {
        "box" => RootKind::Box,
        "system" => RootKind::System,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let scope = match row.get::<_, String>(3)?.as_str() {
        "primary" => RootScope::Primary,
        "additional" => RootScope::Additional,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let origin = match row.get::<_, String>(5)?.as_str() {
        "configured" => RootOrigin::Configured,
        "env" => RootOrigin::Env,
        "catalog" => RootOrigin::Catalog,
        "scan" => RootOrigin::Scan,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let access = match row.get::<_, String>(7)?.as_str() {
        "read_only" => RootAccess::ReadOnly,
        "read_write" => RootAccess::ReadWrite,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(StoreRootRecord {
        id: row.get(0)?,
        harness: row.get(1)?,
        kind,
        scope,
        resolved_path: row.get(4)?,
        origin,
        config_revision: read_revision(row, 6)?,
        access,
        available: row.get(8)?,
    })
}

fn insert_root(
    connection: &rusqlite::Connection,
    record: &StoreRootRecord,
    previous: Option<&str>,
) -> Result<()> {
    connection.execute("INSERT INTO store_root(id,harness,kind,scope,resolved_path,origin,config_revision,access,available,previous_root_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![record.id,record.harness,record.kind.sql(),record.scope.sql(),record.resolved_path,record.origin.sql(),sql_revision(record.config_revision)?,record.access.sql(),record.available,previous]).map_err(root_insert_error)?;
    Ok(())
}

fn root_insert_error(error: rusqlite::Error) -> RecordsError {
    if matches!(&error,rusqlite::Error::SqliteFailure(code,_) if matches!(code.extended_code,1555|2067))
    {
        RecordsError::Conflict
    } else {
        RecordsError::Sqlite(error)
    }
}

impl Database {
    pub fn register_store_root(&self, prepared: &PreparedStoreRoot) -> Result<StoreRootRecord> {
        self.register_root_record(&prepared.record, false)
    }

    pub fn register_additional_system_root(
        &self,
        prepared: &PreparedStoreRoot,
    ) -> Result<StoreRootRecord> {
        self.register_root_record(&prepared.record, true)
    }

    fn register_root_record(
        &self,
        record: &StoreRootRecord,
        additional: bool,
    ) -> Result<StoreRootRecord> {
        record.validate()?;
        if additional {
            record.validate_additional_system_root()?;
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let existing = transaction
            .query_row(ROOT_BY_ID, [&record.id], root_row)
            .optional()?;
        if let Some(existing) = existing {
            existing.validate()?;
            if existing != *record {
                return Err(RecordsError::Conflict);
            }
            transaction.commit()?;
            return Ok(existing);
        }
        insert_root(&transaction, record, None)?;
        transaction.commit()?;
        Ok(record.clone())
    }

    /// Reconfiguration/reconnection appends a fresh binding. It never edits the
    /// previous id or relocates sessions; unavailable bindings remain unavailable.
    pub fn register_store_root_revision(
        &self,
        previous_id: &str,
        expected_revision: u64,
        prepared: &PreparedStoreRoot,
    ) -> Result<StoreRootRecord> {
        validate_key(previous_id)?;
        validate_revision(expected_revision)?;
        let next = &prepared.record;
        next.validate()?;
        if next.id == previous_id
            || next.config_revision
                != expected_revision
                    .checked_add(1)
                    .ok_or(RecordsError::Conflict)?
        {
            return Err(RecordsError::Conflict);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let previous = transaction
            .query_row(ROOT_BY_ID, [previous_id], root_row)
            .optional()?
            .ok_or(RecordsError::NotFound)?;
        previous.validate()?;
        if previous.config_revision != expected_revision
            || previous.harness != next.harness
            || previous.kind != next.kind
            || previous.scope != next.scope
        {
            return Err(RecordsError::Conflict);
        }
        if transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM store_root WHERE id=?1)",
            [&next.id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(RecordsError::Conflict);
        }
        insert_root(&transaction, next, Some(previous_id))?;
        transaction.commit()?;
        Ok(next.clone())
    }

    pub fn store_root(&self, id: &str) -> Result<Option<StoreRootRecord>> {
        validate_key(id)?;
        let connection = self.lock()?;
        let record = connection
            .query_row(ROOT_BY_ID, [id], root_row)
            .optional()?;
        if let Some(record) = &record {
            record.validate()?;
        }
        Ok(record)
    }

    /// Keyset pagination includes unavailable bindings and performs no FS probes.
    pub fn list_store_roots(
        &self,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoreRootRecord>> {
        if limit == 0 || limit > MAX_RECORD_PAGE {
            return Err(RecordsError::Invalid("limite pagina"));
        }
        if let Some(id) = after_id {
            validate_key(id)?;
        }
        let connection = self.lock()?;
        let mut statement = connection.prepare(ROOT_PAGE)?;
        let records = statement
            .query_map(params![after_id.unwrap_or(""), limit as i64], root_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for record in &records {
            record.validate()?;
        }
        Ok(records)
    }

    pub fn mark_store_root_unavailable(
        &self,
        id: &str,
        expected_revision: u64,
    ) -> Result<StoreRootRecord> {
        validate_key(id)?;
        validate_revision(expected_revision)?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let mut record = transaction
            .query_row(ROOT_BY_ID, [id], root_row)
            .optional()?
            .ok_or(RecordsError::NotFound)?;
        record.validate()?;
        if record.config_revision != expected_revision {
            return Err(RecordsError::Conflict);
        }
        transaction.execute(
            "UPDATE store_root SET available=0 WHERE id=?1 AND config_revision=?2",
            params![id, sql_revision(expected_revision)?],
        )?;
        transaction.commit()?;
        record.available = false;
        Ok(record)
    }

    pub fn workspace_record(
        &self,
        project_id: &str,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceRecord>> {
        validate_id(project_id, CanonicalIdKind::Project)?;
        validate_key(workspace_id)?;
        let connection = self.lock()?;
        let record = read_workspace(&connection, project_id, workspace_id)?;
        if let Some(record) = &record {
            record.validate()?;
        }
        Ok(record)
    }

    /// C06 supplies the validated tree. This boundary additionally bounds JSON
    /// and rejects known runtime payload. This method cannot execute a resource.
    pub fn save_workspace(
        &self,
        mut record: WorkspaceRecord,
        expected_revision: u64,
        owner: &WorkspaceOwner,
    ) -> Result<WorkspaceRecord> {
        owner.validate()?;
        if expected_revision >= MAX_SAFE_REVISION {
            return Err(RecordsError::Invalid("revisione"));
        }
        record.revision = expected_revision + 1;
        record.owner = Some(owner.clone());
        record.validate()?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        match read_workspace(&transaction, &record.project_id, &record.workspace_id)? {
            Some(existing) => {
                existing.validate()?;
                if existing.revision != expected_revision {
                    return Err(RecordsError::Conflict);
                }
                if existing
                    .owner
                    .as_ref()
                    .is_some_and(|existing| existing != owner)
                {
                    return Err(RecordsError::OwnedElsewhere);
                }
                transaction.execute("UPDATE workspace SET revision=?3,schema_version=?4,layout_json=?5,owner_window_id=?6,owner_pid=?7,owner_start_time=?8,updated_at=?9 WHERE project_id=?1 AND workspace_id=?2 AND revision=?10",params![record.project_id,record.workspace_id,sql_revision(record.revision)?,record.schema_version,record.layout_json,owner.window_id,owner.pid,owner.start_time,record.updated_at,sql_revision(expected_revision)?])?;
            }
            None => {
                if expected_revision != 0 {
                    return Err(RecordsError::Conflict);
                }
                transaction.execute("INSERT INTO workspace(project_id,workspace_id,revision,schema_version,layout_json,owner_window_id,owner_pid,owner_start_time,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![record.project_id,record.workspace_id,sql_revision(record.revision)?,record.schema_version,record.layout_json,owner.window_id,owner.pid,owner.start_time,record.updated_at])?;
            }
        }
        transaction.commit()?;
        Ok(record)
    }

    pub fn release_workspace(
        &self,
        project_id: &str,
        workspace_id: &str,
        expected_revision: u64,
        owner: &WorkspaceOwner,
    ) -> Result<()> {
        validate_id(project_id, CanonicalIdKind::Project)?;
        validate_key(workspace_id)?;
        validate_revision(expected_revision)?;
        owner.validate()?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let existing = read_workspace(&transaction, project_id, workspace_id)?
            .ok_or(RecordsError::NotFound)?;
        existing.validate()?;
        if existing.revision != expected_revision {
            return Err(RecordsError::Conflict);
        }
        if existing.owner.as_ref() != Some(owner) {
            return Err(RecordsError::OwnedElsewhere);
        }
        transaction.execute("UPDATE workspace SET owner_window_id=NULL,owner_pid=NULL,owner_start_time=NULL WHERE project_id=?1 AND workspace_id=?2 AND revision=?3",params![project_id,workspace_id,sql_revision(expected_revision)?])?;
        transaction.commit()?;
        Ok(())
    }

    /// Explicit recovery only. The caller loads the saved record and computes
    /// its native death proof off-UI/outside SQL. No process is restored or killed.
    /// Only ownership and revision change; the saved layout/time remain exact.
    pub fn recover_workspace(
        &self,
        proof: &recovery::DeadWorkspaceOwnerProof,
        replacement: Option<&WorkspaceOwner>,
    ) -> Result<WorkspaceRecord> {
        if let Some(owner) = replacement {
            owner.validate()?;
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let mut record = read_workspace(&transaction, &proof.project_id, &proof.workspace_id)?
            .ok_or(RecordsError::NotFound)?;
        record.validate()?;
        if !proof.matches(&record) {
            return Err(RecordsError::Conflict);
        }
        if record.revision >= MAX_SAFE_REVISION {
            return Err(RecordsError::Invalid("revisione recupero"));
        }
        let previous = record.owner.as_ref().ok_or(RecordsError::Conflict)?;
        // The owner guard deliberately forbids direct non-null owner changes.
        // Clear then claim inside this SAME write transaction, after the native
        // proof/CAS check; no other connection can observe an unclaimed interval.
        let changed = transaction.execute("UPDATE workspace SET owner_window_id=NULL,owner_pid=NULL,owner_start_time=NULL WHERE project_id=?1 AND workspace_id=?2 AND revision=?3 AND owner_window_id=?4 AND owner_pid=?5 AND owner_start_time=?6",params![record.project_id,record.workspace_id,sql_revision(record.revision)?,previous.window_id,previous.pid,previous.start_time])?;
        if changed != 1 {
            return Err(RecordsError::Conflict);
        }
        let changed = transaction.execute("UPDATE workspace SET revision=?3,owner_window_id=?4,owner_pid=?5,owner_start_time=?6 WHERE project_id=?1 AND workspace_id=?2 AND revision=?7 AND owner_window_id IS NULL AND owner_pid IS NULL AND owner_start_time IS NULL",params![record.project_id,record.workspace_id,sql_revision(record.revision+1)?,replacement.map(|owner| owner.window_id.as_str()),replacement.map(|owner| owner.pid),replacement.map(|owner| owner.start_time.as_str()),sql_revision(record.revision)?])?;
        if changed != 1 {
            return Err(RecordsError::Conflict);
        }
        record.revision += 1;
        record.owner = replacement.cloned();
        transaction.commit()?;
        Ok(record)
    }
}

fn read_workspace(
    connection: &rusqlite::Connection,
    project_id: &str,
    workspace_id: &str,
) -> Result<Option<WorkspaceRecord>> {
    Ok(connection.query_row("SELECT revision,schema_version,CASE WHEN octet_length(layout_json)<=262144 THEN layout_json END,CASE WHEN octet_length(owner_window_id)<=256 THEN owner_window_id END,owner_pid,CASE WHEN octet_length(owner_start_time)<=256 THEN owner_start_time END,updated_at,(owner_window_id IS NULL OR octet_length(owner_window_id)<=256) AND (owner_start_time IS NULL OR octet_length(owner_start_time)<=256) FROM workspace WHERE project_id=?1 AND workspace_id=?2",params![project_id,workspace_id],|row| {
        if !row.get::<_,bool>(7)? { return Err(rusqlite::Error::InvalidQuery); }
        let window_id:Option<String>=row.get(3)?;
        let pid:Option<u32>=row.get(4)?;
        let start_time:Option<String>=row.get(5)?;
        let owner=match(window_id,pid,start_time) { (None,None,None)=>None,(Some(window_id),Some(pid),Some(start_time))=>Some(WorkspaceOwner{window_id,pid,start_time}),_=>return Err(rusqlite::Error::InvalidQuery) };
        Ok(WorkspaceRecord { project_id:project_id.to_owned(),workspace_id:workspace_id.to_owned(),revision:read_revision(row,0)?,schema_version:row.get(1)?,layout_json:row.get(2)?,owner,updated_at:row.get(6)? })
    }).optional()?)
}
