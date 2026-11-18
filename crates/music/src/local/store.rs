use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use rusqlite::{Connection, OptionalExtension as _, params};
use storage::Database;

use crate::{ImportStatus, ImportedEntry, ImportedPlaylistData, LOCAL_PLAYLIST_PREFIX};

pub struct Stored {
    pub id: String,
    pub name: String,
    pub modified_at: i64,
    /// The picture the listener gave the playlist, if any.
    pub cover: Option<PathBuf>,
    /// What the playlist was imported from, when it was.
    pub imported: Option<ImportedRow>,
}

/// The source details of an imported playlist, read beside its local row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedRow {
    pub source: String,
    pub source_id: String,
    pub description: String,
    pub owner: String,
    pub source_url: String,
    pub cover_url: Option<String>,
    pub imported_at: i64,
    pub total: u32,
    pub duplicates: u32,
}

/// Which local favorites table a star lands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Starred {
    Tracks,
    Albums,
    Artists,
}

impl Starred {
    fn table(self) -> &'static str {
        match self {
            Self::Tracks => "favorites",
            Self::Albums => "favorite_albums",
            Self::Artists => "favorite_artists",
        }
    }

    fn column(self) -> &'static str {
        match self {
            Self::Tracks => "track_id",
            Self::Albums => "album_id",
            Self::Artists => "artist_id",
        }
    }
}

pub struct Store {
    database: Database,
}

impl Store {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    fn open(&self) -> Result<rusqlite::Connection> {
        self.database.open().context("cannot open local playlists")
    }

    /// The starred ids of one kind, newest first, with the moment each was starred.
    pub fn starred(&self, kind: Starred) -> Result<Vec<(String, i64)>> {
        let connection = self.open()?;
        let mut query = connection
            .prepare(&format!(
                "SELECT {}, added_at FROM {} ORDER BY added_at DESC",
                kind.column(),
                kind.table()
            ))
            .context("cannot read local favorites")?;
        let rows = query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .context("cannot read local favorites")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("cannot read local favorites")
    }

    /// Local track IDs ordered by play count, then most recent play.
    pub fn most_played(&self, limit: usize) -> Result<Vec<String>> {
        let connection = self.open()?;
        let mut query = connection
            .prepare(
                "SELECT track_id, COUNT(*) as count
                 FROM plays
                 WHERE track_id LIKE 'local:%'
                 GROUP BY track_id
                 ORDER BY count DESC, MAX(played_at) DESC
                 LIMIT ?",
            )
            .context("cannot read most played local tracks")?;
        let rows = query
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .context("cannot read most played local tracks")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("cannot read most played local tracks")
    }

    pub fn set_starred(&self, kind: Starred, id: &str, saved: bool) -> Result<()> {
        let connection = self.open()?;
        match saved {
            true => connection.execute(
                &format!(
                    "INSERT OR REPLACE INTO {} ({}, added_at) VALUES (?, ?)",
                    kind.table(),
                    kind.column()
                ),
                params![id, stamp()],
            ),
            false => connection.execute(
                &format!("DELETE FROM {} WHERE {} = ?", kind.table(), kind.column()),
                params![id],
            ),
        }
        .context("cannot update a local favorite")?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Stored>> {
        let connection = self.open()?;
        let mut query = connection
            .prepare(
                "SELECT p.id, p.name, p.modified_at, c.path,
                        i.source, i.source_id, i.description, i.owner, i.source_url,
                        i.cover_url, i.imported_at, i.total, i.duplicates
                 FROM playlists p
                 LEFT JOIN playlist_covers c ON c.playlist_id = p.id
                 LEFT JOIN imported_playlists i ON i.playlist_id = p.id
                 ORDER BY p.modified_at DESC",
            )
            .context("cannot read local playlists")?;
        let rows = query
            .query_map([], stored)
            .context("cannot read local playlists")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("cannot read local playlists")
    }

    pub fn one(&self, id: &str) -> Result<Stored> {
        self.open()?
            .query_row(
                "SELECT p.id, p.name, p.modified_at, c.path,
                        i.source, i.source_id, i.description, i.owner, i.source_url,
                        i.cover_url, i.imported_at, i.total, i.duplicates
                 FROM playlists p
                 LEFT JOIN playlist_covers c ON c.playlist_id = p.id
                 LEFT JOIN imported_playlists i ON i.playlist_id = p.id
                 WHERE p.id = ?",
                params![id],
                stored,
            )
            .with_context(|| format!("cannot find local playlist {id}"))
    }

    pub fn create(&self, name: &str) -> Result<String> {
        let id = new_id();
        self.open()?
            .execute(
                "INSERT INTO playlists (id, name, modified_at) VALUES (?, ?, ?)",
                params![id, name, stamp()],
            )
            .context("cannot create a local playlist")?;
        Ok(id)
    }

    /// Writes a playlist imported from another service, in one transaction. Importing the same
    /// source playlist again reuses its local row and replaces its entries, so a repeated
    /// request never leaves two copies in the library. Duplicate entries within the source keep
    /// their own positions.
    pub fn import(&self, data: &ImportedPlaylistData) -> Result<String> {
        let mut connection = self.open()?;
        let transaction = connection
            .transaction()
            .context("cannot import the playlist")?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT playlist_id FROM imported_playlists WHERE source = ? AND source_id = ?",
                params![data.source, data.source_id],
                |row| row.get(0),
            )
            .optional()
            .context("cannot look for an imported playlist")?;
        let id = existing.clone().unwrap_or_else(new_id);
        let now = stamp();

        match existing {
            Some(_) => {
                transaction
                    .execute(
                        "UPDATE playlists SET name = ?, modified_at = ? WHERE id = ?",
                        params![data.name, now, id],
                    )
                    .context("cannot refresh an imported playlist")?;
            }
            None => {
                transaction
                    .execute(
                        "INSERT INTO playlists (id, name, modified_at) VALUES (?, ?, ?)",
                        params![id, data.name, now],
                    )
                    .context("cannot create an imported playlist")?;
            }
        }

        transaction
            .execute(
                "INSERT OR REPLACE INTO imported_playlists
                    (playlist_id, source, source_id, description, owner, source_url,
                     cover_url, imported_at, total, duplicates)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    id,
                    data.source,
                    data.source_id,
                    data.description,
                    data.owner,
                    data.source_url,
                    data.cover_url,
                    data.imported_at,
                    data.entries.len() as i64,
                    data.duplicates as i64,
                ],
            )
            .context("cannot remember the imported playlist")?;
        transaction
            .execute(
                "DELETE FROM imported_entries WHERE playlist_id = ?",
                params![id],
            )
            .context("cannot refresh the imported entries")?;

        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO imported_entries
                        (playlist_id, position, source_id, source_url, external_url, title, artists,
                         album, duration_ms, cover, isrc, status, track_id, added_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .context("cannot prepare the imported entries")?;
            for entry in &data.entries {
                insert
                    .execute(params![
                        id,
                        entry.position as i64,
                        entry.source_id,
                        entry.source_url,
                        entry.external_url,
                        entry.title,
                        entry.artists,
                        entry.album,
                        entry.duration.as_millis().min(i64::MAX as u128) as i64,
                        entry.cover,
                        entry.isrc,
                        entry.status.as_str(),
                        entry.track_id,
                        entry.added_at,
                    ])
                    .context("cannot write an imported entry")?;
            }
        }

        transaction.commit().context("cannot import the playlist")?;
        Ok(id)
    }

    pub fn entries(&self, id: &str) -> Result<Vec<ImportedEntry>> {
        let connection = self.open()?;
        let mut query = connection
            .prepare(
                "SELECT position, source_id, source_url, external_url, title, artists, album,
                        duration_ms, cover, isrc, status, track_id, added_at
                 FROM imported_entries WHERE playlist_id = ? ORDER BY position",
            )
            .context("cannot read the imported entries")?;
        let rows = query
            .query_map(params![id], |row| {
                Ok(ImportedEntry {
                    position: row.get::<_, i64>(0)? as u32,
                    source_id: row.get(1)?,
                    source_url: row.get(2)?,
                    external_url: row.get(3)?,
                    title: row.get(4)?,
                    artists: row.get(5)?,
                    album: row.get(6)?,
                    duration: std::time::Duration::from_millis(row.get::<_, i64>(7)?.max(0) as u64),
                    cover: row.get(8)?,
                    isrc: row.get(9)?,
                    status: ImportStatus::parse(&row.get::<_, String>(10)?),
                    track_id: row.get(11)?,
                    added_at: row.get(12)?,
                })
            })
            .context("cannot read the imported entries")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("cannot read the imported entries")
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<()> {
        self.open()?
            .execute(
                "UPDATE playlists SET name = ?, modified_at = ? WHERE id = ?",
                params![name, stamp(), id],
            )
            .context("cannot rename a local playlist")?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let connection = self.open()?;
        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?",
                params![id],
            )
            .context("cannot empty a local playlist")?;
        connection
            .execute(
                "DELETE FROM imported_entries WHERE playlist_id = ?",
                params![id],
            )
            .context("cannot empty an imported playlist")?;
        connection
            .execute(
                "DELETE FROM imported_playlists WHERE playlist_id = ?",
                params![id],
            )
            .context("cannot forget an imported playlist")?;
        connection
            .execute("DELETE FROM playlists WHERE id = ?", params![id])
            .context("cannot delete a local playlist")?;
        Ok(())
    }

    /// Records `cover` as the playlist's own picture, or forgets it when `None`. Answers the
    /// picture it replaces, so the caller can delete that file.
    pub fn set_cover(&self, id: &str, cover: Option<&Path>) -> Result<Option<PathBuf>> {
        let connection = self.open()?;
        let previous = connection
            .query_row(
                "SELECT path FROM playlist_covers WHERE playlist_id = ?",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("cannot read a local playlist cover")?;
        match cover {
            Some(path) => connection.execute(
                "INSERT OR REPLACE INTO playlist_covers (playlist_id, path) VALUES (?, ?)",
                params![id, path.to_string_lossy()],
            ),
            None => connection.execute(
                "DELETE FROM playlist_covers WHERE playlist_id = ?",
                params![id],
            ),
        }
        .context("cannot change a local playlist cover")?;
        touch(&connection, id)?;
        Ok(previous.map(PathBuf::from))
    }

    pub fn add(&self, id: &str, track_id: &str) -> Result<()> {
        let connection = self.open()?;
        let position: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?",
                params![id],
                |row| row.get(0),
            )
            .context("cannot place a track in a local playlist")?;
        connection
            .execute(
                "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position)
                 VALUES (?, ?, ?)",
                params![id, track_id, position],
            )
            .context("cannot add a track to a local playlist")?;
        touch(&connection, id)
    }

    /// Adds `track_ids` to the end of a playlist in order, in one write. A track it holds
    /// already stays where it is.
    pub fn extend(&self, id: &str, track_ids: &[String]) -> Result<()> {
        let mut connection = self.open()?;
        let transaction = connection
            .transaction()
            .context("cannot add tracks to a local playlist")?;
        let start: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?",
                params![id],
                |row| row.get(0),
            )
            .context("cannot place a track in a local playlist")?;
        for (position, track_id) in (start..).zip(track_ids) {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position)
                     VALUES (?, ?, ?)",
                    params![id, track_id, position],
                )
                .context("cannot add a track to a local playlist")?;
        }
        touch(&transaction, id)?;
        transaction
            .commit()
            .context("cannot add tracks to a local playlist")
    }

    pub fn remove(&self, id: &str, track_id: &str) -> Result<()> {
        let connection = self.open()?;
        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?",
                params![id, track_id],
            )
            .context("cannot remove a track from a local playlist")?;
        touch(&connection, id)
    }

    /// Writes `track_ids` as the playlist's whole order, in one write. They have to be the
    /// tracks it already holds, since only their positions change.
    pub fn reorder(&self, id: &str, track_ids: &[String]) -> Result<()> {
        let mut connection = self.open()?;
        let transaction = connection
            .transaction()
            .context("cannot reorder a local playlist")?;
        for (position, track_id) in (0_i64..).zip(track_ids) {
            transaction
                .execute(
                    "UPDATE playlist_tracks SET position = ? WHERE playlist_id = ? AND track_id = ?",
                    params![position, id, track_id],
                )
                .context("cannot reorder a local playlist")?;
        }
        touch(&transaction, id)?;
        transaction
            .commit()
            .context("cannot reorder a local playlist")
    }

    pub fn tracks(&self, id: &str) -> Result<Vec<String>> {
        let connection = self.open()?;
        let mut query = connection
            .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ? ORDER BY position")
            .context("cannot read a local playlist")?;
        let rows = query
            .query_map(params![id], |row| row.get::<_, String>(0))
            .context("cannot read a local playlist")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("cannot read a local playlist")
    }
}

fn touch(connection: &Connection, id: &str) -> Result<()> {
    connection
        .execute(
            "UPDATE playlists SET modified_at = ? WHERE id = ?",
            params![stamp(), id],
        )
        .context("cannot stamp a local playlist")?;
    Ok(())
}

fn minted() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn new_id() -> String {
    format!("{LOCAL_PLAYLIST_PREFIX}{}-{}", stamp(), minted())
}

/// Reads one joined playlist row, with its imported details when it has any.
fn stored(row: &rusqlite::Row<'_>) -> rusqlite::Result<Stored> {
    let source: Option<String> = row.get(4)?;
    let imported = match source {
        Some(source) => Some(ImportedRow {
            source,
            source_id: row.get(5)?,
            description: row.get(6)?,
            owner: row.get(7)?,
            source_url: row.get(8)?,
            cover_url: row.get(9)?,
            imported_at: row.get(10)?,
            total: row.get::<_, i64>(11)?.max(0) as u32,
            duplicates: row.get::<_, i64>(12)?.max(0) as u32,
        }),
        None => None,
    };
    Ok(Stored {
        id: row.get(0)?,
        name: row.get(1)?,
        modified_at: row.get(2)?,
        cover: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
        imported,
    })
}

fn stamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
