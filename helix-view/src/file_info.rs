use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::{Connection, Result};
use helix_core::Selection;
use log::{debug, info};

pub struct FileInfoDb {
    conn: Option<Connection>,
    enabled: bool,
}

#[derive(Debug)]
pub struct FilePosition {
    pub line: usize,
    pub column: usize,
}

impl FileInfoDb {
    pub fn new(enabled: bool) -> Self {
        if !enabled {
            info!("FileInfoDb::new - Feature disabled");
            return Self { conn: None, enabled: false };
        }

        info!("FileInfoDb::new - Feature enabled, initializing database");
        let db_path = Self::get_db_path();
        info!("FileInfoDb::new - Database path: {:?}", db_path);
        let conn = Self::init_database(db_path);

        if conn.is_some() {
            info!("FileInfoDb::new - Database connection established");
        } else {
            info!("FileInfoDb::new - Failed to establish database connection");
        }

        Self { conn, enabled }
    }

    fn get_db_path() -> PathBuf {
        let config_dir = helix_loader::config_dir();
        config_dir.join("info.sqlite")
    }

    fn init_database(path: PathBuf) -> Option<Connection> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }

        let conn = Connection::open(path).ok()?;

        // Create table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS fileinfo (
                filepath TEXT PRIMARY KEY,
                line INTEGER NOT NULL,
                column INTEGER NOT NULL,
                last_modified INTEGER NOT NULL
            )",
            [],
        ).ok()?;

        Some(conn)
    }

    pub fn save_position(&mut self, path: &Path, selection: &Selection, text: &helix_core::Rope) -> Result<()> {
        if !self.enabled || self.conn.is_none() {
            debug!("FileInfoDb::save_position - not enabled or no connection");
            return Ok(());
        }

        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let filepath = canonical.to_string_lossy();

        // Get primary selection cursor position
        let text_slice = text.slice(..);
        let primary = selection.primary();
        let cursor = primary.cursor(text_slice);
        let position = helix_core::coords_at_pos(text_slice, cursor);
        let (line, column) = (position.row, position.col);

        info!("FileInfoDb::save_position - Saving position for {}: line={}, column={}",
              filepath, line, column);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if let Some(conn) = &self.conn {
            conn.execute(
                "INSERT OR REPLACE INTO fileinfo (filepath, line, column, last_modified)
                 VALUES (?1, ?2, ?3, ?4)",
                (&filepath, line as i64, column as i64, timestamp),
            )?;
            debug!("FileInfoDb::save_position - Successfully saved to database");
        }

        Ok(())
    }

    pub fn load_position(&self, path: &Path) -> Option<FilePosition> {
        if !self.enabled {
            debug!("FileInfoDb::load_position - not enabled");
            return None;
        }

        info!("FileInfoDb::load_position - Attempting to load position for {:?}", path);

        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                debug!("FileInfoDb::load_position - Failed to canonicalize path: {}", e);
                return None;
            }
        };
        let filepath = canonical.to_string_lossy();
        debug!("FileInfoDb::load_position - Canonical path: {}", filepath);

        let conn = self.conn.as_ref()?;

        let mut stmt = match conn.prepare(
            "SELECT line, column FROM fileinfo WHERE filepath = ?1"
        ) {
            Ok(s) => s,
            Err(e) => {
                debug!("FileInfoDb::load_position - Failed to prepare statement: {}", e);
                return None;
            }
        };

        let position = match stmt.query_row([&filepath], |row| {
            Ok(FilePosition {
                line: row.get::<_, i64>(0)? as usize,
                column: row.get::<_, i64>(1)? as usize,
            })
        }) {
            Ok(pos) => {
                info!("FileInfoDb::load_position - Loaded position: line={}, column={}", pos.line, pos.column);
                pos
            }
            Err(e) => {
                debug!("FileInfoDb::load_position - No saved position found: {}", e);
                return None;
            }
        };

        Some(position)
    }
}