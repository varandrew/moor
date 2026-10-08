use super::Database;

pub fn run_migrations(db: &Database) -> Result<(), String> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS profiles (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS mcp_servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            connection_type TEXT NOT NULL CHECK(connection_type IN ('stdio', 'http')),
            command TEXT,
            args TEXT,
            url TEXT,
            env TEXT,
            headers TEXT,
            working_dir TEXT,
            auto_start INTEGER NOT NULL DEFAULT 0,
            sort_order INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'stopped' CHECK(status IN ('stopped', 'starting', 'running', 'error')),
            error_message TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS profile_servers (
            profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
            server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
            enabled INTEGER NOT NULL DEFAULT 1,
            disabled_tools TEXT NOT NULL DEFAULT '[]',
            PRIMARY KEY (profile_id, server_id)
        );

        CREATE TABLE IF NOT EXISTS tool_discoveries (
            server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
            tool_name TEXT NOT NULL,
            exposed_name TEXT NOT NULL,
            description TEXT,
            input_schema TEXT,
            discovered_at TEXT NOT NULL,
            PRIMARY KEY (server_id, tool_name)
        );

        CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            profile_id TEXT REFERENCES profiles(id),
            server_id TEXT REFERENCES mcp_servers(id),
            tool_name TEXT NOT NULL,
            arguments TEXT,
            result TEXT,
            error TEXT,
            duration_ms INTEGER,
            agent_info TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_tool_name ON audit_logs(tool_name);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_server_id ON audit_logs(server_id);
        CREATE INDEX IF NOT EXISTS idx_tool_discoveries_server_id ON tool_discoveries(server_id);
        CREATE INDEX IF NOT EXISTS idx_tool_discoveries_exposed_name ON tool_discoveries(exposed_name);

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;

    ensure_column(db, "tool_discoveries", "exposed_name", "TEXT")?;
    ensure_column(db, "mcp_servers", "headers", "TEXT")?;
    ensure_column(
        db,
        "mcp_servers",
        "auto_start",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        db,
        "mcp_servers",
        "sort_order",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    backfill_server_sort_order(db)?;
    normalize_audit_timestamps(db)?;

    Ok(())
}

fn normalize_audit_timestamps(db: &Database) -> Result<(), String> {
    db.transaction(|conn| {
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if version >= 1 {
            return Ok(());
        }
        let mut statement = conn
            .prepare("SELECT id, timestamp FROM audit_logs")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?;
        let mut updates = Vec::new();
        for row in rows {
            let (id, timestamp) = row.map_err(|error| error.to_string())?;
            let normalized = super::audit_log_repo::normalize_timestamp(&timestamp)
                .map_err(|error| format!("Cannot migrate audit log {id}: {error}"))?;
            if timestamp != normalized {
                updates.push((id, normalized));
            }
        }
        for (id, timestamp) in updates {
            conn.execute(
                "UPDATE audit_logs SET timestamp = ?1 WHERE id = ?2",
                rusqlite::params![timestamp, id],
            )
            .map_err(|error| error.to_string())?;
        }
        conn.execute_batch("PRAGMA user_version = 1")
            .map_err(|error| error.to_string())
    })
}

fn ensure_column(db: &Database, table: &str, column: &str, definition: &str) -> Result<(), String> {
    let all_cols = db.query_all(&format!("PRAGMA table_info({table})"), &[], |row| {
        row.get::<_, String>(1)
    })?;
    if all_cols.iter().any(|c| c == column) {
        return Ok(());
    }
    db.run(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        &[],
    )
}

fn backfill_server_sort_order(db: &Database) -> Result<(), String> {
    let rows = db.query_all(
        "SELECT id, sort_order FROM mcp_servers ORDER BY created_at DESC, id ASC",
        &[],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    if rows.len() <= 1 {
        return Ok(());
    }
    let needs_backfill = rows
        .iter()
        .map(|(_, o)| *o)
        .collect::<std::collections::HashSet<_>>()
        .len()
        == 1;
    if !needs_backfill {
        return Ok(());
    }
    db.transaction(|conn| {
        for (index, (id, _)) in rows.iter().enumerate() {
            conn.execute(
                "UPDATE mcp_servers SET sort_order = ?1 WHERE id = ?2",
                rusqlite::params![&(index as i64), id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn temp_db() -> Database {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("moor-migrations-{ts}.db"));
        Database::open(&path).expect("open db")
    }

    #[test]
    fn audit_timestamp_migration_preserves_precision_and_time_index() {
        let db = temp_db();
        run_migrations(&db).unwrap();
        db.exec("PRAGMA user_version = 0").unwrap();
        for (id, timestamp) in [
            ("offset", "2026-01-01T08:00:00.123456789+08:00"),
            ("utc", "2026-01-01T00:00:00Z"),
        ] {
            db.run(
                "INSERT INTO audit_logs (id, timestamp, tool_name) VALUES (?1, ?2, 'search')",
                &[&id, &timestamp],
            )
            .unwrap();
        }
        run_migrations(&db).unwrap();
        let timestamps = db
            .query_all(
                "SELECT timestamp FROM audit_logs ORDER BY timestamp",
                &[],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(
            timestamps,
            [
                "2026-01-01T00:00:00.000000000Z",
                "2026-01-01T00:00:00.123456789Z"
            ]
        );
        assert_eq!(
            db.query_one("PRAGMA user_version", &[], |row| row.get::<_, i64>(0))
                .unwrap(),
            Some(1)
        );
        let plan = db.query_all("EXPLAIN QUERY PLAN SELECT * FROM audit_logs WHERE timestamp >= ?1 AND timestamp < ?2 ORDER BY timestamp DESC", &[&timestamps[0], &timestamps[1]], |row| row.get::<_, String>(3)).unwrap();
        assert!(plan
            .iter()
            .any(|step| step.contains("idx_audit_logs_timestamp")));
        run_migrations(&db).unwrap();
        assert_eq!(
            db.query_all(
                "SELECT timestamp FROM audit_logs ORDER BY timestamp",
                &[],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            timestamps
        );
    }

    #[test]
    fn audit_timestamp_migration_keeps_invalid_history_and_rolls_back_updates() {
        let db = temp_db();
        run_migrations(&db).unwrap();
        db.exec("PRAGMA user_version = 0;
            INSERT INTO audit_logs (id, timestamp, tool_name) VALUES ('first', '2026-01-01T00:00:00Z', 'search'), ('invalid', 'garbage', 'search');").unwrap();
        assert!(run_migrations(&db).unwrap_err().contains("invalid"));
        assert_eq!(
            db.query_one("PRAGMA user_version", &[], |row| row.get::<_, i64>(0))
                .unwrap(),
            Some(0)
        );
        assert_eq!(
            db.query_one(
                "SELECT timestamp FROM audit_logs WHERE id = 'first'",
                &[],
                |row| row.get::<_, String>(0)
            )
            .unwrap()
            .as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        db.exec("UPDATE audit_logs SET timestamp = '2026-01-02T00:00:00Z' WHERE id = 'invalid';
            CREATE TRIGGER reject_timestamp_update BEFORE UPDATE OF timestamp ON audit_logs WHEN NEW.id = 'invalid' BEGIN SELECT RAISE(ABORT, 'Test migration failure'); END;").unwrap();
        assert!(run_migrations(&db).is_err());
        assert_eq!(
            db.query_one(
                "SELECT timestamp FROM audit_logs WHERE id = 'first'",
                &[],
                |row| row.get::<_, String>(0)
            )
            .unwrap()
            .as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!(
            db.query_one("PRAGMA user_version", &[], |row| row.get::<_, i64>(0))
                .unwrap(),
            Some(0)
        );
    }

    #[test]
    fn backfills_sort_order_newest_created_first() {
        let db = temp_db();
        db.exec(
            "CREATE TABLE mcp_servers (
                id TEXT PRIMARY KEY, name TEXT NOT NULL,
                connection_type TEXT NOT NULL CHECK(connection_type IN ('stdio','http')),
                command TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );",
        )
        .expect("create table");
        for (id, created) in [
            ("old", "2026-01-01T00:00:00.000Z"),
            ("new", "2026-01-03T00:00:00.000Z"),
            ("middle", "2026-01-02T00:00:00.000Z"),
        ] {
            db.run(
                "INSERT INTO mcp_servers (id, name, connection_type, command, created_at, updated_at) VALUES (?1, ?1, 'stdio', 'node', ?2, ?2)",
                &[&id, &created],
            )
            .expect("insert row");
        }

        run_migrations(&db).expect("migrate");

        let rows = db
            .query_all(
                "SELECT id, sort_order FROM mcp_servers ORDER BY sort_order ASC",
                &[],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .expect("query");
        assert_eq!(
            rows,
            vec![
                ("new".to_string(), 0),
                ("middle".to_string(), 1),
                ("old".to_string(), 2),
            ]
        );
    }
}
