mod cache;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context as _, Result};
use rusqlite::Connection;

pub use cache::Cache;

#[cfg(test)]
mod tests {
    use super::*;

    fn columns(connection: &Connection) -> Vec<String> {
        columns_of(connection, "imported_entries")
    }

    fn columns_of(connection: &Connection, table: &str) -> Vec<String> {
        connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn an_old_imported_entries_table_gains_the_external_link_and_keeps_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE imported_entries (
                        playlist_id TEXT NOT NULL,
                        position INTEGER NOT NULL,
                        source_id TEXT,
                        source_url TEXT,
                        title TEXT NOT NULL,
                        artists TEXT NOT NULL,
                        album TEXT NOT NULL,
                        duration_ms INTEGER NOT NULL,
                        cover TEXT,
                        isrc TEXT,
                        status TEXT NOT NULL,
                        track_id TEXT,
                        added_at INTEGER,
                        PRIMARY KEY (playlist_id, position)
                    );
                    INSERT INTO imported_entries
                        (playlist_id, position, title, artists, album, duration_ms, status)
                    VALUES ('list', 0, 'Old song', 'Artist', 'Album', 180000, 'available');",
                )
                .unwrap();
        }

        let database = Database::at(path);
        {
            let connection = database.open().unwrap();
            assert!(
                columns(&connection)
                    .iter()
                    .any(|column| column == "external_url")
            );
            let title: String = connection
                .query_row("SELECT title FROM imported_entries", [], |row| row.get(0))
                .unwrap();
            assert_eq!(title, "Old song");
        }
        // A second open in the same process must not try to add the column again.
        database.open().unwrap();
    }

    #[test]
    fn a_fresh_database_already_has_the_external_link() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::at(dir.path().join("fresh.sqlite"));
        let connection = database.open().unwrap();
        assert!(
            columns(&connection)
                .iter()
                .any(|column| column == "external_url")
        );
    }

    #[test]
    fn the_cache_database_opens_without_the_imported_tables() {
        let dir = tempfile::tempdir().unwrap();
        // The migration runs for every database; the cache schema has no imported tables,
        // and asking it to add a column to one used to fail every cache read.
        let cache = Cache::at(dir.path().join("cache.sqlite"));
        cache.write("key", "value", 1).unwrap();
        assert_eq!(
            cache.read("key", |text| text.to_owned()).unwrap(),
            Some("value".to_owned())
        );
    }

    #[test]
    fn an_old_imported_playlist_row_gains_the_generated_cover_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE imported_playlists (
                        playlist_id TEXT PRIMARY KEY,
                        source TEXT NOT NULL,
                        source_id TEXT NOT NULL,
                        description TEXT NOT NULL,
                        owner TEXT NOT NULL,
                        source_url TEXT NOT NULL,
                        cover_url TEXT,
                        imported_at INTEGER NOT NULL,
                        total INTEGER NOT NULL,
                        duplicates INTEGER NOT NULL DEFAULT 0
                    );
                    INSERT INTO imported_playlists
                        (playlist_id, source, source_id, description, owner, source_url,
                         imported_at, total)
                    VALUES ('list', 'csv', 'abc', '', '', '', 1000, 3);",
                )
                .unwrap();
        }

        let database = Database::at(path);
        let connection = database.open().unwrap();
        assert!(
            columns_of(&connection, "imported_playlists")
                .iter()
                .any(|column| column == "cover_generated")
        );
        // An existing row is not retroactively called generated: its picture may be the
        // listener's own, and a later artwork pass must not overwrite it.
        let generated: i64 = connection
            .query_row(
                "SELECT cover_generated FROM imported_playlists",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(generated, 0);
    }
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS app_state (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS plays (
        scope TEXT NOT NULL,
        provider TEXT NOT NULL,
        track_id TEXT NOT NULL,
        played_at INTEGER NOT NULL,
        name TEXT NOT NULL,
        playable INTEGER NOT NULL,
        artists TEXT NOT NULL,
        artist_refs TEXT NOT NULL,
        album TEXT NOT NULL,
        album_id TEXT,
        cover TEXT,
        duration_ms INTEGER NOT NULL,
        explicit INTEGER NOT NULL,
        PRIMARY KEY (scope, provider, track_id, played_at)
    );
    CREATE INDEX IF NOT EXISTS plays_scope_time
        ON plays (scope, played_at DESC);
    CREATE TABLE IF NOT EXISTS flags (
        key TEXT PRIMARY KEY,
        value INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS playlists (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        modified_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS playlist_tracks (
        playlist_id TEXT NOT NULL,
        track_id TEXT NOT NULL,
        position INTEGER NOT NULL,
        PRIMARY KEY (playlist_id, track_id)
    );
    CREATE INDEX IF NOT EXISTS playlist_tracks_order
        ON playlist_tracks (playlist_id, position);
    CREATE TABLE IF NOT EXISTS playlist_covers (
        playlist_id TEXT PRIMARY KEY,
        path TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS imported_playlists (
        playlist_id TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        source_id TEXT NOT NULL,
        description TEXT NOT NULL,
        owner TEXT NOT NULL,
        source_url TEXT NOT NULL,
        cover_url TEXT,
        imported_at INTEGER NOT NULL,
        total INTEGER NOT NULL,
        duplicates INTEGER NOT NULL DEFAULT 0,
        cover_generated INTEGER NOT NULL DEFAULT 0
    );
    CREATE UNIQUE INDEX IF NOT EXISTS imported_playlists_source
        ON imported_playlists (source, source_id);
    CREATE TABLE IF NOT EXISTS imported_entries (
        playlist_id TEXT NOT NULL,
        position INTEGER NOT NULL,
        source_id TEXT,
        source_url TEXT,
        external_url TEXT,
        title TEXT NOT NULL,
        artists TEXT NOT NULL,
        album TEXT NOT NULL,
        duration_ms INTEGER NOT NULL,
        cover TEXT,
        isrc TEXT,
        status TEXT NOT NULL,
        track_id TEXT,
        added_at INTEGER,
        PRIMARY KEY (playlist_id, position)
    );
    CREATE INDEX IF NOT EXISTS imported_entries_order
        ON imported_entries (playlist_id, position);
    CREATE TABLE IF NOT EXISTS favorites (
        track_id TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS favorite_albums (
        album_id TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS favorite_artists (
        artist_id TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS downloads (
        provider TEXT NOT NULL,
        track_id TEXT NOT NULL,
        single INTEGER NOT NULL,
        done INTEGER NOT NULL,
        added_at INTEGER NOT NULL,
        PRIMARY KEY (provider, track_id)
    );
    CREATE TABLE IF NOT EXISTS downloaded_sets (
        provider TEXT NOT NULL,
        kind TEXT NOT NULL,
        set_id TEXT NOT NULL,
        value TEXT NOT NULL,
        added_at INTEGER NOT NULL,
        PRIMARY KEY (provider, kind, set_id)
    );";

#[derive(Clone)]
pub struct Database {
    path: PathBuf,
    ready: Arc<Mutex<bool>>,
}

impl Database {
    pub fn standard() -> Self {
        let data = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("soundy");
        Self {
            path: data.join("state.sqlite"),
            ready: Arc::new(Mutex::new(false)),
        }
    }

    pub fn at(path: PathBuf) -> Self {
        Self {
            path,
            ready: Arc::new(Mutex::new(false)),
        }
    }

    pub fn open(&self) -> Result<Connection> {
        let connection = connect(&self.path, SCHEMA, &self.ready, "app state")?;
        Ok(connection)
    }
}

/// Adds the columns a schema change introduced to an existing database. A fresh database
/// already has them, so this is a no-op there. Each column is added in place, so an old
/// database keeps every row it had.
fn ensure_import_columns(connection: &Connection) -> Result<()> {
    add_missing_columns(connection, "imported_entries", &[("external_url", "TEXT")])?;
    add_missing_columns(
        connection,
        "imported_playlists",
        &[("cover_generated", "INTEGER NOT NULL DEFAULT 0")],
    )
}

fn add_missing_columns(
    connection: &Connection,
    table: &str,
    wanted: &[(&str, &str)],
) -> Result<()> {
    let mut query = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("cannot inspect the {table} table"))?;
    let present: Vec<String> = query
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("cannot read the {table} columns"))?
        .collect::<rusqlite::Result<_>>()
        .with_context(|| format!("cannot read the {table} columns"))?;
    // This runs for every database this crate opens, and the cache database has neither
    // imported table. An empty column list means the table is not here; the schema that
    // creates it always creates the current columns, so there is nothing to add.
    if present.is_empty() {
        return Ok(());
    }
    for (column, definition) in wanted {
        if present.iter().any(|name| name == column) {
            continue;
        }
        connection
            .execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )
            .with_context(|| format!("cannot add the {column} column"))?;
    }
    Ok(())
}

/// Opens `path`, applying `schema` the first time this process opens it. `what` names the
/// database in the errors a caller sees.
pub(crate) fn connect(
    path: &Path,
    schema: &str,
    ready: &Mutex<bool>,
    what: &str,
) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create the {what} directory"))?;
    }
    let connection = Connection::open(path).with_context(|| format!("cannot open {what}"))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .with_context(|| format!("cannot configure {what}"))?;
    let mut ready = ready
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !*ready {
        connection
            .execute_batch(schema)
            .with_context(|| format!("cannot prepare {what}"))?;
        // A database made before imported rows carried an external page keeps its rows; the
        // column is added in place rather than rebuilding the table. A fresh schema already
        // has it, and the check runs once per process, not on every query.
        ensure_import_columns(&connection)?;
        *ready = true;
    }
    Ok(connection)
}
