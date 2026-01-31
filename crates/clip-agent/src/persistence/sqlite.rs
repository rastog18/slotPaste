//! SQLite persistence for slots. DB path: macOS ~/Library/Application Support/Slotpaste/slotpaste.db,
//! other ~/.slotpaste/slotpaste.db.

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;

const CREATE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS slots (
    slot_key TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    updated_at INTEGER NOT NULL
)"#;

/// Returns DB path and creates parent dirs. macOS: ~/Library/Application Support/Slotpaste/slotpaste.db;
/// other: ~/.slotpaste/slotpaste.db.
pub fn db_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
    let (dir, file) = if cfg!(target_os = "macos") {
        (
            format!("{}/Library/Application Support/Slotpaste", home),
            "slotpaste.db",
        )
    } else {
        (format!("{}/.slotpaste", home), "slotpaste.db")
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all: {}", e))?;
    Ok(PathBuf::from(dir).join(file))
}

/// Open DB and create table if not exists.
pub fn init_db() -> Result<Connection, String> {
    let path = db_path()?;
    let conn = Connection::open(&path).map_err(|e| format!("open db: {}", e))?;
    conn.execute(CREATE_TABLE, [])
        .map_err(|e| format!("create table: {}", e))?;
    Ok(conn)
}

/// Load all slots: slot_key -> content.
pub fn load_all(conn: &Connection) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT slot_key, content FROM slots")
        .map_err(|e| format!("prepare load: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("query: {}", e))?;
    let mut out = HashMap::new();
    for row in rows {
        let (k, v) = row.map_err(|e| format!("row: {}", e))?;
        out.insert(k, v);
    }
    Ok(out)
}

/// Upsert slot by key. updated_at = unix epoch seconds.
pub fn upsert_slot(conn: &Connection, slot_key: &str, content: &str) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("time: {}", e))?
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO slots (slot_key, content, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(slot_key) DO UPDATE SET content = ?2, updated_at = ?3",
        rusqlite::params![slot_key, content, now],
    )
    .map_err(|e| format!("upsert: {}", e))?;
    Ok(())
}
