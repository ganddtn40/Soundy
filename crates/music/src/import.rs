//! Reading a playlist from another service or a local list, and the vocabulary every importer
//! shares.
//!
//! Nothing here reaches the network on its own. The Spotify Web API client lives in
//! [`crate::spotify::web`]; the public catalogue lookups live in [`crate::audius`] and
//! [`crate::jamendo`]. This module holds the link and file parsers, the status model and the
//! title/artist matching rules, so persistence and the screen can be tested without an
//! account, a file dialog or a network.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use percent_encoding::percent_decode_str;

use crate::Track;
use crate::models::{ImportedEntry, ImportedPlaylistData};

/// How much two recordings of the same song may differ in length and still be the same take.
/// Spotify rounds to the second, and a local file's tag may round differently.
pub const SAME_RECORDING: Duration = Duration::from_secs(5);

/// The largest text file the importer will read. A playlist list is never close to this; the
/// cap keeps a mistaken audio or disk image from being pulled into memory.
pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

/// What made an import fail, in a form the dialog can explain without reading an English log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportFailure {
    /// The text is not a playlist link, URI or track list.
    InvalidLink,
    /// The link is a Spotify playlist, but Spotify does not publish its tracks without an
    /// authorized session. The dialog offers a file or a pasted list instead.
    TracklistUnavailable,
    /// Spotify is not the provider the app is playing through, so there is no session to ask.
    /// Only the optional Spotify Web API path uses this.
    SpotifyNotConnected,
    /// The token was refused. Signing in again is the fix for the optional path.
    Unauthorized,
    /// The playlist is private, or the account may not read it. Soundy never bypasses this.
    Forbidden,
    NotFound,
    /// Spotify asked the client to wait. The wait was honoured before giving up.
    RateLimited {
        seconds: u64,
    },
    Cancelled,
    /// The chosen file has no tracks, or is not one of the supported list formats.
    EmptySource,
    UnsupportedFile(String),
    FileTooLarge,
    File(String),
    Network(String),
    Other(String),
}

impl fmt::Display for ImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLink => formatter.write_str("that is not a Spotify playlist link"),
            Self::TracklistUnavailable => formatter.write_str(
                "Spotify does not publish this playlist's tracks without an authorized Spotify session; import a CSV, M3U or pasted list instead",
            ),
            Self::SpotifyNotConnected => {
                formatter.write_str("importing from a Spotify link needs an authorized Spotify session, but Soundy has none")
            }
            Self::Unauthorized => formatter.write_str("Spotify did not accept the saved session"),
            Self::Forbidden => formatter.write_str(
                "Spotify did not allow this account to read the playlist; private playlists need the owner to sign in",
            ),
            Self::NotFound => formatter.write_str("Spotify could not find that playlist"),
            Self::RateLimited { seconds } => {
                write!(formatter, "Spotify is rate limiting requests; try again in {seconds}s")
            }
            Self::Cancelled => formatter.write_str("the import was cancelled"),
            Self::EmptySource => formatter.write_str("the list has no tracks to import"),
            Self::UnsupportedFile(name) => {
                write!(formatter, "{name} is not an M3U, M3U8, CSV or text playlist")
            }
            Self::FileTooLarge => formatter.write_str("the playlist file is too large to import"),
            Self::File(reason) => formatter.write_str(reason),
            Self::Network(reason) => write!(formatter, "cannot reach the service: {reason}"),
            Self::Other(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for ImportFailure {}

/// A Spotify playlist named by a link, URI or bare id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpotifyLink {
    pub id: String,
}

impl SpotifyLink {
    pub fn url(&self) -> String {
        format!("https://open.spotify.com/playlist/{}", self.id)
    }
}

/// Reads `https://open.spotify.com/playlist/<id>`, its `/intl-xx/` and `/embed/` forms, the
/// `spotify:playlist:<id>` URI and the older `spotify:user:<name>:playlist:<id>` one. A query
/// string, fragment and trailing slash are ignored. Any other Spotify link is refused rather
/// than adapted, so a track or album link never imports the wrong thing.
pub fn parse_spotify_link(input: &str) -> Result<SpotifyLink, ImportFailure> {
    let text = input.trim();
    if text.is_empty() {
        return Err(ImportFailure::InvalidLink);
    }

    let id = if let Some(rest) = text.strip_prefix("spotify:playlist:").or_else(|| {
        let rest = text.strip_prefix("spotify:user:")?;
        let (_, playlist) = rest.split_once(":playlist:")?;
        Some(playlist)
    }) {
        clean_id(rest)
    } else {
        let without_scheme = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"))
            .unwrap_or(text);
        let host = without_scheme
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .trim_end_matches(':');
        if !matches!(
            host,
            "open.spotify.com" | "play.spotify.com" | "spotify.com"
        ) {
            return Err(ImportFailure::InvalidLink);
        }
        let path = without_scheme
            .split_once('/')
            .map(|(_, path)| path)
            .unwrap_or_default();
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let mut parts = path.split('/').filter(|part| !part.is_empty());
        let mut found = None;
        while let Some(part) = parts.next() {
            if part == "playlist" {
                found = parts.next();
                break;
            }
            // `intl-de` and `embed` are path prefixes Spotify puts in front of the id.
            if !part.starts_with("intl-") && part != "embed" {
                break;
            }
        }
        found.and_then(clean_id)
    };

    id.map(|id| SpotifyLink { id })
        .ok_or(ImportFailure::InvalidLink)
}

fn clean_id(raw: &str) -> Option<String> {
    let id = raw.split(['?', '#', '/']).next().unwrap_or_default().trim();
    let valid =
        (10..=64).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_alphanumeric());
    valid.then(|| id.to_owned())
}

/// A recording as its source listed it, before matching. A Spotify page fills every field; a
/// CSV row may name only a title; an M3U line also carries the path the file names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceEntry {
    pub id: Option<String>,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub duration: Duration,
    pub cover: Option<String>,
    pub isrc: Option<String>,
    /// The year the release came out, when the export named a release date. It is kept for
    /// artwork and metadata matching only; a row never claims a date its source did not give.
    pub release_year: Option<i32>,
    pub url: Option<String>,
    /// A local file an M3U named. It is matched by path before title and artist are considered.
    pub local_path: Option<PathBuf>,
    /// Spotify's own answer to whether this account may stream it.
    pub playable: bool,
    /// Whether Spotify offers only a clip.
    pub preview: bool,
    /// A file Spotify knows from the user's disk rather than its catalogue.
    pub local: bool,
    pub added_at: Option<i64>,
}

/// A whole playlist as the source listed it, before matching.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourcePlaylist {
    /// `spotify`, `csv`, `m3u` or `manual`.
    pub source: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub cover: Option<String>,
    pub source_url: String,
    /// The count the source reports, which may exceed `entries` if one was skipped.
    pub total: usize,
    /// Rows that were present but could not be read as a track, as opposed to blank lines.
    pub skipped: usize,
    pub entries: Vec<SourceEntry>,
}

impl SourcePlaylist {
    /// Turns the parsed playlist into entries carrying their status and matched track.
    /// `local_matches` is aligned with `entries` and holds the id of the listener's own file
    /// that is this recording, when one is; the two match slices are aligned too, holding the
    /// active provider's playable match and the permitted public catalogue's page. A match is
    /// never guessed: an absent or versioned recording stays unavailable.
    pub fn into_import(
        self,
        local_matches: &[Option<String>],
        provider_matches: &[Option<Track>],
        external_matches: &[Option<String>],
        imported_at: i64,
    ) -> ImportedPlaylistData {
        let mut duplicates = 0_u32;
        let mut seen: HashSet<String> = HashSet::new();
        let mut entries = Vec::with_capacity(self.entries.len());

        for (position, entry) in self.entries.into_iter().enumerate() {
            let key = entry_key(&entry);
            if !seen.insert(key) {
                duplicates += 1;
            }

            let mut external_url = None;
            let (status, track_id) =
                if let Some(found) = local_matches.get(position).cloned().flatten() {
                    (crate::models::ImportStatus::Personal, Some(found))
                } else if let Some(found) = provider_matches.get(position).cloned().flatten() {
                    (crate::models::ImportStatus::Available, found.id.clone())
                } else if let Some(url) = external_matches.get(position).cloned().flatten() {
                    external_url = Some(url);
                    (crate::models::ImportStatus::External, None)
                } else if self.source == "spotify" && entry.playable && entry.id.is_some() {
                    (
                        crate::models::ImportStatus::Available,
                        entry.id.as_ref().map(|id| format!("spotify:track:{id}")),
                    )
                } else if entry.preview {
                    (crate::models::ImportStatus::Preview, None)
                } else if entry.url.is_some() {
                    (crate::models::ImportStatus::External, None)
                } else {
                    (crate::models::ImportStatus::Unavailable, None)
                };
            let source_url = entry.url.clone();
            external_url = external_url.filter(|url| Some(url) != source_url.as_ref());

            entries.push(ImportedEntry {
                position: position as u32,
                source_id: entry.id,
                source_url,
                external_url,
                title: entry.title,
                artists: entry.artists,
                album: entry.album,
                duration: entry.duration,
                cover: entry.cover,
                isrc: entry.isrc,
                status,
                track_id,
                added_at: entry.added_at,
            });
        }

        ImportedPlaylistData {
            source: self.source,
            source_id: self.id,
            name: self.name,
            description: self.description,
            owner: self.owner,
            source_url: self.source_url,
            cover_url: self.cover,
            imported_at,
            entries,
            duplicates,
            skipped: self.skipped,
        }
    }
}

fn entry_key(entry: &SourceEntry) -> String {
    entry
        .id
        .clone()
        .or_else(|| entry.isrc.clone())
        .or_else(|| {
            entry
                .local_path
                .as_ref()
                .map(|path| path.display().to_string())
        })
        .unwrap_or_else(|| format!("{}|{}", key(&entry.title), key(&entry.artists)))
}

/// Reads an M3U or M3U8 list. A relative file entry is read against `folder`, the folder the
/// list sits in. `#EXTINF` supplies the duration and an `Artist - Title` line; a bare path uses
/// the file's stem as its title. A path outside the library is kept as an unavailable row, and
/// an HTTP(S) entry is kept as an external row with its own URL as the source link.
pub fn parse_m3u(bytes: &[u8], folder: &Path, name: &str) -> Result<SourcePlaylist, ImportFailure> {
    let text = decode(bytes);
    let mut entries = Vec::new();
    let mut pending: Option<(Duration, String)> = None;
    let mut skipped = 0;

    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            let (duration, label) = info.split_once(',').unwrap_or((info, ""));
            pending = Some((
                parse_duration(duration).unwrap_or_default(),
                label.trim().to_owned(),
            ));
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with("http://") || line.starts_with("https://") {
            // A stream URL is kept as an external row: its label names the station or track,
            // and the URL is the source link the row opens. It is never guessed to be a file.
            let (duration, label) = pending.take().unwrap_or_default();
            let (artists, title) = split_credit(&label);
            let title = match title.is_empty() {
                true => url_title(line),
                false => title,
            };
            if title.is_empty() {
                skipped += 1;
                continue;
            }
            entries.push(SourceEntry {
                id: Some(line.to_owned()),
                title,
                artists,
                duration,
                url: Some(line.to_owned()),
                ..Default::default()
            });
            continue;
        }
        let Some(path) = m3u_path(line, folder) else {
            skipped += 1;
            pending = None;
            continue;
        };
        let (duration, label) = pending.take().unwrap_or_default();
        let (artists, title) = split_credit(&label);
        let title = match title.is_empty() {
            true => path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            false => title,
        };
        if title.is_empty() {
            skipped += 1;
            continue;
        }
        entries.push(SourceEntry {
            id: Some(path.display().to_string()),
            title,
            artists,
            duration,
            local_path: Some(path),
            ..Default::default()
        });
    }

    finish(SourcePlaylist {
        source: "m3u".to_owned(),
        id: String::new(),
        name: name.to_owned(),
        description: String::new(),
        owner: String::new(),
        cover: None,
        source_url: String::new(),
        total: entries.len() + skipped,
        skipped,
        entries,
    })
}

/// Reads a CSV playlist. The header may be in any order or case and may use the column names
/// Spotify, Exportify, Soundiiz, TuneMyMusic and a hand-made sheet write: `Track Name`, `Artist
/// Name(s)`, `Album Name`, `Track Duration (ms)`, `Album Image URL`, `ISRC`, `Track URI`, `Added
/// At` and `Release Date` all map to their field. Comma, semicolon and tab files are read, with
/// or without a byte order mark. A headerless file is read in the classic
/// `Title,Artist,Album,Duration` order. A row with no title is counted as skipped, never
/// invented; a missing duration stays unknown rather than becoming zero.
pub fn parse_csv(bytes: &[u8], name: &str) -> Result<SourcePlaylist, ImportFailure> {
    let text = decode(bytes);
    let delimiter = Delimiter::of(&text);
    let rows = csv_rows(&text, delimiter);
    let mut rows = rows
        .into_iter()
        .filter(|row| row.iter().any(|cell| !cell.trim().is_empty()));
    let Some(first) = rows.next() else {
        return Err(ImportFailure::EmptySource);
    };

    let header = csv_header(&first);
    let mut entries = Vec::new();
    let mut skipped = 0;
    let mut read = |row: &[String]| {
        let title = match &header {
            Some(header) => cell(row, header.title),
            None => cell(row, 0),
        }
        .trim();
        if title.is_empty() {
            skipped += 1;
            return;
        }

        let (artists, album, duration, cover, isrc, source, url, added_at, release) = match &header
        {
            Some(header) => (
                cell(row, header.artists),
                cell(row, header.album),
                (cell(row, header.duration), header.unit),
                cell(row, header.cover),
                cell(row, header.isrc),
                cell(row, header.source),
                cell(row, header.url),
                cell(row, header.added_at),
                cell(row, header.release),
            ),
            None => (
                cell(row, 1),
                cell(row, 2),
                (cell(row, 3), Unit::Seconds),
                "",
                "",
                "",
                "",
                "",
                "",
            ),
        };

        let (source_id, source_url) = match spotify_track(source) {
            (Some(id), url) => (Some(id), url),
            // A `Track URL` column may hold the Spotify page instead of an id column.
            _ => spotify_track(url),
        };
        let url = source_url.or_else(|| web_url(url));
        entries.push(SourceEntry {
            id: source_id,
            title: title.to_owned(),
            artists: artists.trim().to_owned(),
            album: album.trim().to_owned(),
            duration: duration_value(duration.0, duration.1),
            cover: web_url(cover),
            isrc: non_empty(isrc),
            release_year: release_year(release),
            url,
            added_at: crate::iso_8601_to_epoch(non_empty(added_at).as_deref()),
            ..Default::default()
        });
    };

    if header.is_none() {
        read(&first);
    }
    for row in rows {
        read(&row);
    }

    finish(SourcePlaylist {
        source: "csv".to_owned(),
        id: String::new(),
        name: name.to_owned(),
        description: String::new(),
        owner: String::new(),
        cover: None,
        source_url: String::new(),
        total: entries.len() + skipped,
        skipped,
        entries,
    })
}

/// Reads a pasted track list. Every non-empty line is a track. A pipe-separated line follows
/// the CSV order: `Title | Artist | Album | Duration`. An `Artist - Title` line is the common
/// shorthand, and a line with no separator is a title. Blank lines and `#` comments are ignored.
pub fn parse_manual(text: &str, name: &str) -> Result<SourcePlaylist, ImportFailure> {
    let mut entries = Vec::new();
    let mut skipped = 0;
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cells = split_pipes(line);
        let (title, artists, album, duration) = match cells.len() {
            0 => continue,
            1 => {
                let (artists, title) = split_credit(&cells[0]);
                (title, artists, String::new(), String::new())
            }
            _ => (
                cells.first().cloned().unwrap_or_default(),
                cells.get(1).cloned().unwrap_or_default(),
                cells.get(2).cloned().unwrap_or_default(),
                cells.get(3).cloned().unwrap_or_default(),
            ),
        };
        if title.trim().is_empty() {
            skipped += 1;
            continue;
        }
        entries.push(SourceEntry {
            title: title.trim().to_owned(),
            artists: artists.trim().to_owned(),
            album: album.trim().to_owned(),
            duration: parse_duration(&duration).unwrap_or_default(),
            ..Default::default()
        });
    }

    finish(SourcePlaylist {
        source: "manual".to_owned(),
        id: String::new(),
        name: name.to_owned(),
        description: String::new(),
        owner: String::new(),
        cover: None,
        source_url: String::new(),
        total: entries.len() + skipped,
        skipped,
        entries,
    })
}

fn finish(playlist: SourcePlaylist) -> Result<SourcePlaylist, ImportFailure> {
    if playlist.entries.is_empty() {
        return Err(ImportFailure::EmptySource);
    }
    Ok(playlist)
}

/// The text of a playlist file. An `.m3u8`/CSV is UTF-8, with or without a byte order mark,
/// while an older `.m3u` may be Latin-1, read byte for byte when the file is not valid UTF-8.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|&byte| char::from(byte)).collect(),
    }
}

fn m3u_path(line: &str, folder: &Path) -> Option<PathBuf> {
    let path = if let Some(rest) = line
        .get(.."file://".len())
        .filter(|scheme| scheme.eq_ignore_ascii_case("file://"))
        .map(|_| &line["file://".len()..])
    {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode_str(rest).decode_utf8_lossy().into_owned();
        let without_drive_slash = match decoded.as_bytes() {
            [b'/', letter, b':', ..] if cfg!(windows) && letter.is_ascii_alphabetic() => {
                &decoded[1..]
            }
            _ => decoded.as_str(),
        };
        separated(without_drive_slash)
    } else {
        if line.contains("://") {
            return None;
        }
        separated(line)
    };
    Some(folder.join(path))
}

fn separated(path: &str) -> PathBuf {
    let path = match cfg!(windows) {
        true => path.replace('/', "\\"),
        false if !path.contains('/') => path.replace('\\', "/"),
        false => path.to_owned(),
    };
    PathBuf::from(path)
}

/// The readable name of a stream URL: its last path segment, percent-decoded and with the file
/// extension dropped. It is only used when an `#EXTINF` line names nothing.
fn url_title(url: &str) -> String {
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let last = clean
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let decoded = percent_decode_str(last).decode_utf8_lossy();
    let stem = match decoded.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => decoded.as_ref(),
    };
    stem.replace(['_', '+'], " ").trim().to_owned()
}

/// `Artist - Title`, the credit form every list writes. A title that itself holds ` - ` keeps
/// the part after the first separator; the artist is the part before it.
fn split_credit(line: &str) -> (String, String) {
    match line.split_once(" - ") {
        Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => {
            (artist.trim().to_owned(), title.trim().to_owned())
        }
        _ => (String::new(), line.trim().to_owned()),
    }
}

fn split_pipes(line: &str) -> Vec<String> {
    line.split('|').map(|cell| cell.trim().to_owned()).collect()
}

fn cell(row: &[String], slot: usize) -> &str {
    row.get(slot).map(String::as_str).unwrap_or_default()
}

/// How a CSV column separates cells. A sheet and an export disagree on this often enough that
/// the first row decides it before any cell is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delimiter {
    Comma,
    Semicolon,
    Tab,
}

impl Delimiter {
    /// The delimiter the first line uses most, ignoring separators inside quoted cells.
    fn of(text: &str) -> Self {
        let mut counts = [0_usize; 3];
        let mut quoted = false;
        for letter in text.chars() {
            match letter {
                '"' => quoted = !quoted,
                '\n' | '\r' if !quoted => break,
                ',' if !quoted => counts[0] += 1,
                ';' if !quoted => counts[1] += 1,
                '\t' if !quoted => counts[2] += 1,
                _ => {}
            }
        }
        match (counts[0], counts[1], counts[2]) {
            (comma, semicolon, tab) if tab > comma && tab > semicolon => Self::Tab,
            (comma, semicolon, _) if semicolon > comma => Self::Semicolon,
            _ => Self::Comma,
        }
    }

    fn letter(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Tab => '\t',
        }
    }
}

/// Whether a duration column counts seconds or milliseconds. Exportify writes
/// `Track Duration (ms)`; a hand-made sheet writes `3:45` or a whole number of seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Seconds,
    Milliseconds,
}

#[derive(Clone, Copy)]
struct Header {
    title: usize,
    artists: usize,
    album: usize,
    duration: usize,
    unit: Unit,
    cover: usize,
    isrc: usize,
    source: usize,
    url: usize,
    added_at: usize,
    release: usize,
}

impl Header {
    const MISSING: Self = Self {
        title: usize::MAX,
        artists: usize::MAX,
        album: usize::MAX,
        duration: usize::MAX,
        unit: Unit::Seconds,
        cover: usize::MAX,
        isrc: usize::MAX,
        source: usize::MAX,
        url: usize::MAX,
        added_at: usize::MAX,
        release: usize::MAX,
    };
}

/// The lowercase, punctuation-normalized form a header is compared in. A byte order mark left
/// in front of the first header cell and the underscores some exports use are removed.
fn header_name(value: &str) -> String {
    let cleaned = value
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .replace('_', " ");
    let mut name = String::with_capacity(cleaned.len());
    let mut space = false;
    for letter in cleaned.chars() {
        if letter.is_alphanumeric() || letter == '(' || letter == ')' {
            name.push(letter);
            space = false;
        } else if !space && !name.is_empty() {
            name.push(' ');
            space = true;
        }
    }
    name.trim().to_owned()
}

fn find_column(names: &[String], candidates: &[&str]) -> usize {
    candidates
        .iter()
        .find_map(|candidate| names.iter().position(|name| name == candidate))
        .unwrap_or(usize::MAX)
}

fn csv_header(row: &[String]) -> Option<Header> {
    let names: Vec<String> = row.iter().map(|value| header_name(value)).collect();
    let mut header = Header::MISSING;
    header.title = find_column(
        &names,
        &["track name", "title", "song title", "track title"],
    );
    if header.title == usize::MAX {
        return None;
    }
    // `Artist Name(s)` is the credited line; `Album Artist` is only a fallback for a sheet
    // that does not carry one.
    header.artists = find_column(
        &names,
        &[
            "artist name(s)",
            "artist name",
            "artist names",
            "artists",
            "artist",
            "artist(s)",
            "creator",
            "album artist",
        ],
    );
    header.album = find_column(&names, &["album name", "album"]);
    let milliseconds = [
        "track duration (ms)",
        "duration (ms)",
        "duration ms",
        "length (ms)",
        "time (ms)",
    ];
    match find_column(&names, &milliseconds) {
        index if index != usize::MAX => {
            header.duration = index;
            header.unit = Unit::Milliseconds;
        }
        _ => {
            header.duration =
                find_column(&names, &["track duration", "duration", "length", "time"]);
        }
    }
    header.cover = find_column(
        &names,
        &[
            "album image url",
            "album cover url",
            "album cover",
            "cover url",
            "cover art",
            "artwork url",
            "artwork",
            "image url",
            "cover",
            "image",
        ],
    );
    header.isrc = find_column(&names, &["isrc", "isrc code", "isrcs"]);
    header.source = find_column(
        &names,
        &[
            "track uri",
            "spotify id",
            "spotifyid",
            "spotify - id",
            "track id",
            "id",
        ],
    );
    header.url = find_column(&names, &["track url", "external url", "url", "link"]);
    header.added_at = find_column(&names, &["added at", "date added", "added"]);
    header.release = find_column(
        &names,
        &["release date", "album release date", "release year", "year"],
    );
    // A headerless list may begin with a title that reads like a column name. If the cell in
    // the duration column is a length, the row is data and the file has no header.
    if header.duration != usize::MAX
        && !duration_value(cell(row, header.duration), header.unit).is_zero()
    {
        return None;
    }
    Some(header)
}

/// A small CSV reader. It returns one row per record, with quoted cells unescaped and embedded
/// newlines kept. It deliberately accepts a final row without a newline.
fn csv_rows(text: &str, delimiter: Delimiter) -> Vec<Vec<String>> {
    let separator = delimiter.letter();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(letter) = chars.next() {
        match (quoted, letter) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            (true, '"') => quoted = false,
            (true, letter) => cell.push(letter),
            (false, '"') if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            (false, letter) if letter == separator => row.push(std::mem::take(&mut cell)),
            (false, '\r') if chars.peek() == Some(&'\n') => {
                chars.next();
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, '\n') => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, letter) => cell.push(letter),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// A non-blank cell, trimmed, or nothing. Empty cells in an export are written as `""` and
/// sometimes as a stray space.
fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// An HTTP(S) artwork or page URL. A cell that says `None` or names a local path is not one.
fn web_url(value: &str) -> Option<String> {
    let value = value.trim();
    (value.starts_with("https://") || value.starts_with("http://")).then(|| value.to_owned())
}

/// The Spotify track id and its canonical page from an export's `Track URI`, `Spotify - id` or
/// `open.spotify.com/track/…` cell. A non-Spotify URL is left to the plain URL column.
fn spotify_track(value: &str) -> (Option<String>, Option<String>) {
    let value = value.trim();
    let id = value
        .strip_prefix("spotify:track:")
        .or_else(|| {
            let (_, rest) = value.split_once("open.spotify.com/track/")?;
            Some(rest)
        })
        .map(|rest| {
            rest.split(['?', '#', '/'])
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .filter(|id| {
            (10..=64).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
        });
    match id {
        Some(id) => {
            let url = format!("https://open.spotify.com/track/{id}");
            (Some(id), Some(url))
        }
        None => (None, web_url(value)),
    }
}

/// The year a release date names, keeping only a plausible four-digit year.
fn release_year(value: &str) -> Option<i32> {
    let value = value.trim();
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    digits
        .get(..4)
        .and_then(|year| year.parse().ok())
        .filter(|year| (1900..=2100).contains(year))
}

/// A duration cell in the unit its header declared. A clock reading (`3:45`) is always a clock,
/// whatever the column is called.
fn duration_value(value: &str, unit: Unit) -> Duration {
    let value = value.trim();
    if value.is_empty() {
        return Duration::ZERO;
    }
    if value.contains(':') {
        return parse_duration(value).unwrap_or_default();
    }
    match unit {
        Unit::Seconds => parse_duration(value).unwrap_or_default(),
        Unit::Milliseconds => value
            .parse::<u64>()
            .or_else(|_| value.parse::<f64>().map(|value| value.round() as u64))
            .map(Duration::from_millis)
            .unwrap_or_default(),
    }
}

/// Reads `3:45`, `1:03:45` or a whole number of seconds. A missing or unreadable duration is
/// unknown, never zero-as-a-fact, so matching treats it as unknown.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse::<u64>().ok().map(Duration::from_secs);
    }
    let mut parts = text.split(':');
    let first = parts.next()?.trim().parse::<u64>().ok()?;
    let second = parts.next()?.trim().parse::<u64>().ok()?;
    let third = parts
        .next()
        .and_then(|part| part.trim().parse::<u64>().ok());
    if parts.next().is_some() || second >= 60 {
        return None;
    }
    let seconds = match third {
        Some(third) => first * 3600 + second * 60 + third,
        None => first * 60 + second,
    };
    Some(Duration::from_secs(seconds))
}

/// The local files whose title and credited artist agree with `entry`, whatever their length.
/// An importer reads their ISRC tags only for this short list rather than the whole library.
/// A file an M3U named by path is a candidate even when its tags do not agree.
pub fn local_candidates<'a>(entry: &SourceEntry, local: &'a [Track]) -> Vec<&'a Track> {
    let mut found: Vec<&Track> = Vec::new();
    if let Some(track) = entry
        .local_path
        .as_deref()
        .and_then(|path| local.iter().find(|track| local_path_is(track, path)))
    {
        found.push(track);
    }

    let title = key(&entry.title);
    if title.is_empty() {
        return found;
    }
    let artists = split_artists(&entry.artists);
    for track in local {
        if found.iter().any(|kept| std::ptr::eq(*kept, track)) {
            continue;
        }
        if key(&track.name) == title && same_artist(&artists, &split_artists(&track.artists)) {
            found.push(track);
        }
    }
    found
}

/// The best local file for `entry`: an M3U's exact path first, then the album's own cut, then
/// the first exact title/artist/duration match, then an ISRC match among title/artist
/// candidates. A remix, live take or cover differs in its title and is never substituted.
pub fn match_local<'a>(
    entry: &SourceEntry,
    local: &'a [Track],
    isrc: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<&'a Track> {
    let candidates = local_candidates(entry, local);
    if candidates.is_empty() {
        return None;
    }
    if let Some(path) = entry.local_path.as_deref()
        && let Some(track) = candidates
            .iter()
            .copied()
            .find(|track| local_path_is(track, path))
    {
        return Some(track);
    }

    let album = key(&entry.album);
    let timed: Vec<&Track> = candidates
        .iter()
        .copied()
        .filter(|track| close(track.duration, entry.duration))
        .collect();
    if let Some(track) = timed
        .iter()
        .copied()
        .find(|track| !album.is_empty() && key(&track.album) == album)
    {
        return Some(track);
    }
    if let Some(track) = timed.first().copied() {
        return Some(track);
    }

    // The lengths disagree, so the only honest answer left is the recording's own ISRC.
    let wanted = entry.isrc.as_deref()?;
    candidates.iter().copied().find(|track| {
        track
            .id
            .as_deref()
            .and_then(&mut *isrc)
            .is_some_and(|found| found.eq_ignore_ascii_case(wanted))
    })
}

/// The words a provider search should be asked for: the recording's title, then its artist
/// when one is known.
pub fn search_query(entry: &SourceEntry) -> String {
    match entry.artists.trim().is_empty() {
        true => entry.title.trim().to_owned(),
        false => format!("{} {}", entry.title.trim(), entry.artists.trim()),
    }
}

/// Whether `track` is the recording `entry` names, by a provider's or a public catalogue's
/// own title, artist, length and ISRC. Version words (`remix`, `live`, `acoustic`, `edit`) are
/// kept in the title key, so a different take is never accepted as a substitute.
pub fn track_matches(entry: &SourceEntry, track: &Track) -> bool {
    same_recording(entry, &track.name, &track.artists, track.duration, None)
}

/// The same check for a catalogue answer that is not a [`Track`] yet, such as Audius's JSON.
pub fn same_recording(
    entry: &SourceEntry,
    title: &str,
    artists: &str,
    duration: Duration,
    isrc: Option<&str>,
) -> bool {
    if let (Some(wanted), Some(found)) = (entry.isrc.as_deref(), isrc) {
        if wanted.eq_ignore_ascii_case(found) {
            return true;
        }
    }
    title_matches(&entry.title, title, &entry.artists)
        && same_artist(&split_artists(&entry.artists), &split_artists(artists))
        && close(entry.duration, duration)
}

fn local_path_is(track: &Track, path: &Path) -> bool {
    track
        .id
        .as_deref()
        .and_then(|id| id.strip_prefix(crate::LOCAL_TRACK_PREFIX))
        .is_some_and(|found| Path::new(found) == path)
}

/// Whether two lengths can be the same recording. A missing length on either side counts as
/// unknown, not as a mismatch.
fn close(left: Duration, right: Duration) -> bool {
    let left = left.as_secs();
    let right = right.as_secs();
    left == 0 || right == 0 || left.abs_diff(right) <= SAME_RECORDING.as_secs()
}

fn title_matches(wanted: &str, found: &str, artists: &str) -> bool {
    let wanted = key(wanted);
    let found = key(found);
    if wanted.is_empty() || found.is_empty() {
        return false;
    }
    if wanted == found {
        return true;
    }
    // Audius and several uploaders put the artist in the title (`Artist - Title`). It is the
    // same recording when the artist is one of the entry's credited artists and the rest is
    // the exact title.
    split_artists(artists).into_iter().any(|artist| {
        found
            .strip_prefix(&format!("{artist} "))
            .is_some_and(|rest| rest == wanted)
    })
}

/// Normalizes a title or an album for comparison: lower case, `&` read as `and`, punctuation
/// reduced to spaces, a leading or trailing `feat.` credit dropped and whitespace collapsed.
/// Version words such as `live`, `remix`, `acoustic` and `edit` are deliberately kept.
pub fn key(text: &str) -> String {
    let mut text = deunicode::deunicode(text).to_lowercase();
    text = text.replace('&', " and ");
    // A guest credit names a performer, not a different recording.
    if let Some(start) = text.find("feat.") {
        text.truncate(start);
    }
    if let Some(start) = text.find("featuring ") {
        text.truncate(start);
    }
    let mut cleaned = String::with_capacity(text.len());
    let mut space = false;
    for letter in text.chars() {
        if letter.is_alphanumeric() {
            cleaned.push(letter);
            space = false;
        } else if !space {
            cleaned.push(' ');
            space = true;
        }
    }
    cleaned.trim().to_owned()
}

/// The names in a provider's artist line, lower-cased and normalized. It splits on commas, the
/// ampersand and the word `and`, which every provider uses for a list.
fn split_artists(line: &str) -> Vec<String> {
    line.split([',', '&', ';'])
        .flat_map(|part| part.split(" and "))
        .map(key)
        .filter(|name| !name.is_empty())
        .collect()
}

/// Whether the recording's primary artist is one of the file's credited artists. Comparing the
/// whole list would fail when one provider calls a record "A, B" and the other "A".
fn same_artist(wanted: &[String], found: &[String]) -> bool {
    match wanted.first() {
        Some(first) => found.iter().any(|name| name == first),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str, artists: &str, album: &str, seconds: u64) -> Track {
        Track {
            id: Some(format!("local:{name}:{artists}")),
            name: name.to_owned(),
            artists: artists.to_owned(),
            album: album.to_owned(),
            duration: Duration::from_secs(seconds),
            ..Default::default()
        }
    }

    #[test]
    fn reads_every_playlist_link_spotify_hands_out() {
        for link in [
            "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
            "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc",
            "https://open.spotify.com/intl-de/playlist/37i9dQZF1DXcBWIGoYBM5M",
            "https://open.spotify.com/embed/playlist/37i9dQZF1DXcBWIGoYBM5M",
            "http://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M/",
            "open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
            "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
            "spotify:user:spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
        ] {
            assert_eq!(
                parse_spotify_link(link).unwrap().id,
                "37i9dQZF1DXcBWIGoYBM5M",
                "{link}"
            );
        }
    }

    #[test]
    fn refuses_other_links_and_other_services() {
        for link in [
            "",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "https://open.spotify.com/album/0JGOiO34nwfUdDrD612dOp",
            "https://open.spotify.com/artist/06HL4z0CvFAxyc27GXpf02",
            "https://music.youtube.com/playlist?list=OLAK5uy",
            "https://example.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
            "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
        ] {
            assert_eq!(
                parse_spotify_link(link),
                Err(ImportFailure::InvalidLink),
                "{link}"
            );
        }
    }

    #[test]
    fn title_keys_keep_versions_and_drop_guest_credits() {
        assert_eq!(key("Song (feat. Guest)"), "song");
        assert_eq!(key("Song - Live"), "song live");
        assert_eq!(key("Song (Remix)"), "song remix");
        assert_eq!(key("Beyoncé & Friends"), "beyonce and friends");
    }

    #[test]
    fn matches_an_album_cut_within_the_length_tolerance() {
        let local = vec![
            track("Song", "Artist", "Single", 201),
            track("Song", "Artist", "Album", 203),
        ];
        let entry = SourceEntry {
            title: "Song".into(),
            artists: "Artist".into(),
            album: "Album".into(),
            duration: Duration::from_secs(203),
            ..Default::default()
        };
        let found = match_local(&entry, &local, &mut |_| None).unwrap();
        assert_eq!(found.album, "Album");
    }

    #[test]
    fn never_matches_a_remix_live_take_or_cover() {
        let local = vec![
            track("Song - Live", "Artist", "Live", 210),
            track("Song (Remix)", "Artist", "Remixes", 205),
            track("Song", "Another Artist", "Tribute", 202),
        ];
        let entry = SourceEntry {
            title: "Song".into(),
            artists: "Artist".into(),
            album: "Album".into(),
            duration: Duration::from_secs(202),
            ..Default::default()
        };
        assert!(match_local(&entry, &local, &mut |_| None).is_none());
    }

    #[test]
    fn falls_back_to_isrc_when_lengths_disagree() {
        let mut local = vec![track("Song", "Artist", "Album", 500)];
        local[0].id = Some("local:isrc".into());
        let entry = SourceEntry {
            title: "Song".into(),
            artists: "Artist".into(),
            album: "Album".into(),
            duration: Duration::from_secs(202),
            isrc: Some("USABC1234567".into()),
            ..Default::default()
        };
        let found = match_local(&entry, &local, &mut |_| Some("USABC1234567".into()));
        assert_eq!(
            found.and_then(|track| track.id.as_deref()),
            Some("local:isrc")
        );
    }

    #[test]
    fn keeps_duplicate_entries_and_calls_them_duplicates() {
        let playlist = SourcePlaylist {
            id: "list".into(),
            name: "List".into(),
            source: "spotify".into(),
            source_url: "https://open.spotify.com/playlist/list".into(),
            entries: vec![
                SourceEntry {
                    id: Some("one".into()),
                    title: "One".into(),
                    playable: true,
                    ..Default::default()
                },
                SourceEntry {
                    id: Some("one".into()),
                    title: "One".into(),
                    playable: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let import = playlist.into_import(&[], &[], &[], 0);
        assert_eq!(import.entries.len(), 2);
        assert_eq!(import.duplicates, 1);
        assert!(import.entries.iter().all(|entry| entry.status.playable()));
    }

    #[test]
    fn parses_csv_headers_quotes_and_large_lists() {
        let mut csv = "Title,Artist,Album,Duration\n\"Song, One\",Artist,A \"quoted\" album,3:45\n"
            .to_owned();
        for index in 0..500 {
            csv.push_str(&format!(
                "Song {index},Artist {index},Album,4:{:02}\n",
                index % 60
            ));
        }
        let playlist = parse_csv(csv.as_bytes(), "List").unwrap();
        assert_eq!(playlist.entries.len(), 501);
        assert_eq!(playlist.entries[0].title, "Song, One");
        assert_eq!(playlist.entries[0].album, "A \"quoted\" album");
        assert_eq!(playlist.entries[0].duration, Duration::from_secs(225));
        assert_eq!(playlist.entries[500].title, "Song 499");
    }

    #[test]
    fn reads_csv_columns_in_any_order_and_without_a_header() {
        let reordered = parse_csv(b"artist,title,duration\nArtist,Song,2:05\n", "List").unwrap();
        assert_eq!(reordered.entries[0].title, "Song");
        assert_eq!(reordered.entries[0].artists, "Artist");
        assert_eq!(reordered.entries[0].duration, Duration::from_secs(125));

        let headless = parse_csv(b"Song,Artist,Album,90\n", "List").unwrap();
        assert_eq!(headless.entries[0].title, "Song");
        assert_eq!(headless.entries[0].duration, Duration::from_secs(90));
    }

    #[test]
    fn skips_a_csv_row_without_a_title_but_keeps_the_rest() {
        let playlist = parse_csv(b"Title,Artist\nOne,Artist\n,No title\nTwo,Artist\n", "List");
        let playlist = playlist.unwrap();
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.skipped, 1);
        assert_eq!(playlist.entries[1].title, "Two");
    }

    #[test]
    fn reads_an_exportify_csv_with_artwork_ids_and_millisecond_durations() {
        let csv = "\u{feff}\"Track Name\",\"Artist Name(s)\",\"Album Name\",\"Track Duration (ms)\",\"Album Image URL\",\"ISRC\",\"Track URI\",\"Added At\",\"Release Date\"\n\
                   \"Hardwired\",\"Rameses B\",\"Hardwired\",\"302000\",\"https://i.scdn.co/image/abc\",\"GB2LD1700001\",\"spotify:track:4uLU6hMCjMI75M1A2tKUQC\",\"2024-05-04T10:11:12Z\",\"2017-09-22\"\n";
        let playlist = parse_csv(csv.as_bytes(), "Exportify").unwrap();
        let entry = &playlist.entries[0];
        assert_eq!(entry.title, "Hardwired");
        assert_eq!(entry.artists, "Rameses B");
        assert_eq!(entry.album, "Hardwired");
        assert_eq!(entry.duration, Duration::from_secs(302));
        assert_eq!(entry.cover.as_deref(), Some("https://i.scdn.co/image/abc"));
        assert_eq!(entry.isrc.as_deref(), Some("GB2LD1700001"));
        assert_eq!(entry.id.as_deref(), Some("4uLU6hMCjMI75M1A2tKUQC"));
        assert_eq!(
            entry.url.as_deref(),
            Some("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")
        );
        assert_eq!(entry.added_at, Some(1_714_817_472));
        assert_eq!(entry.release_year, Some(2017));
    }

    #[test]
    fn reads_semicolon_and_tab_exports_and_a_spotify_web_link() {
        let semicolons = parse_csv(
            b"Track Name;Artist Name(s);Album Name;Duration\nSong;Artist;Album;3:45\n",
            "Sheet",
        )
        .unwrap();
        assert_eq!(semicolons.entries[0].title, "Song");
        assert_eq!(semicolons.entries[0].duration, Duration::from_secs(225));

        let tabs = parse_csv(
            "Track Name\tArtist Name(s)\tAlbum Name\tTrack Duration (ms)\nSong\tArtist\tAlbum\t201000\n"
                .as_bytes(),
            "Sheet",
        )
        .unwrap();
        assert_eq!(tabs.entries[0].duration, Duration::from_secs(201));

        let linked = parse_csv(
            b"Title,Artist,Track URL\nSong,Artist,https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC\n",
            "Links",
        )
        .unwrap();
        assert_eq!(
            linked.entries[0].id.as_deref(),
            Some("4uLU6hMCjMI75M1A2tKUQC")
        );
        assert_eq!(
            linked.entries[0].url.as_deref(),
            Some("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")
        );
        assert!(linked.entries[0].cover.is_none());
    }

    #[test]
    fn keeps_a_missing_export_duration_unknown_instead_of_zero_as_a_fact() {
        let playlist = parse_csv(
            b"Track Name,Artist Name(s),Album Name,Track Duration (ms)\nSong,Artist,Album,\n",
            "Sheet",
        )
        .unwrap();
        assert_eq!(playlist.entries[0].duration, Duration::ZERO);
    }

    #[test]
    fn parses_manual_lists_in_both_common_forms() {
        let playlist = parse_manual(
            "Artist One - Song One\nSong Two | Artist Two | Album Two | 3:30\n# a comment\nSong Three\n",
            "Pasted",
        )
        .unwrap();
        assert_eq!(playlist.entries.len(), 3);
        assert_eq!(playlist.entries[0].title, "Song One");
        assert_eq!(playlist.entries[0].artists, "Artist One");
        assert_eq!(playlist.entries[1].title, "Song Two");
        assert_eq!(playlist.entries[1].duration, Duration::from_secs(210));
        assert_eq!(playlist.entries[2].title, "Song Three");
    }

    #[test]
    fn reads_m3u_extinf_and_relative_paths_in_order() {
        let bytes = b"#EXTM3U\n#EXTINF:225,Artist - Song One\nmusic/one.mp3\n#EXTINF:-1,Other - Song Two\nC:\\Music\\two.flac\n";
        let playlist = parse_m3u(bytes, Path::new("C:\\List"), "List").unwrap();
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.entries[0].title, "Song One");
        assert_eq!(playlist.entries[0].duration, Duration::from_secs(225));
        assert_eq!(
            playlist.entries[0].local_path.as_deref(),
            Some(Path::new("C:\\List\\music\\one.mp3"))
        );
        assert_eq!(playlist.entries[1].title, "Song Two");
    }

    #[test]
    fn keeps_an_m3u_stream_url_as_an_external_row_with_its_link() {
        let bytes = b"#EXTM3U\n#EXTINF:-1,Rameses B - Hardwired\nhttps://example.com/music/hardwired.mp3\nhttps://example.com/stream/Deep_Focus.mp3\n";
        let playlist = parse_m3u(bytes, Path::new("C:\\List"), "Streams").unwrap();
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.entries[0].title, "Hardwired");
        assert_eq!(playlist.entries[0].artists, "Rameses B");
        assert_eq!(
            playlist.entries[0].url.as_deref(),
            Some("https://example.com/music/hardwired.mp3")
        );
        assert!(playlist.entries[0].local_path.is_none());
        // A bare URL falls back to its own readable name.
        assert_eq!(playlist.entries[1].title, "Deep Focus");

        let import = playlist.into_import(&[], &[], &[], 0);
        assert_eq!(
            import.entries[0].status,
            crate::models::ImportStatus::External
        );
        assert_eq!(
            import.entries[0].source_url.as_deref(),
            Some("https://example.com/music/hardwired.mp3")
        );
        assert!(import.entries[0].external_url.is_none());
    }

    #[test]
    fn an_empty_list_is_not_imported() {
        assert_eq!(
            parse_csv(b"Title,Artist\n", "Empty"),
            Err(ImportFailure::EmptySource)
        );
        assert_eq!(
            parse_manual("\n# nothing\n", "Empty"),
            Err(ImportFailure::EmptySource)
        );
    }

    #[test]
    fn matches_a_catalogue_answer_that_puts_the_artist_in_the_title() {
        let entry = SourceEntry {
            title: "Hardwired".into(),
            artists: "Rameses B".into(),
            duration: Duration::from_secs(302),
            ..Default::default()
        };
        assert!(same_recording(
            &entry,
            "Rameses B - Hardwired",
            "Rameses B",
            Duration::from_secs(302),
            None,
        ));
        assert!(!same_recording(
            &entry,
            "Rameses B - Hardwired (Remix)",
            "Rameses B",
            Duration::from_secs(302),
            None,
        ));
        assert!(!same_recording(
            &entry,
            "Hardwired",
            "Cover Band",
            Duration::from_secs(302),
            None,
        ));
    }
}
