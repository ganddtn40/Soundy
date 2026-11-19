//! Artwork and missing metadata for an imported playlist.
//!
//! A CSV export carries names and lengths but often no picture, and a pasted list carries even
//! less. This module asks two public catalogues that need no account — the iTunes Search API and
//! Deezer's public API — for the artwork of the exact recording the row names. Nothing is
//! accepted on a title alone: the credited artist must agree, and either the length must be
//! within [`SAME_RECORDING`] or, when the source gave no length, the album must agree too. A
//! remix, live take or cover therefore keeps the placeholder instead of borrowing unrelated art.
//!
//! Rows are grouped by album first, so a 500-row list of forty albums costs forty lookups rather
//! than five hundred. Only albums that still have no picture fall back to a per-recording search,
//! and both phases have a hard request cap so an import cannot turn into an unbounded crawl.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::import::{SAME_RECORDING, SourceEntry, key};

const ITUNES: &str = "https://itunes.apple.com";
const DEEZER: &str = "https://api.deezer.com";
const TIMEOUT: Duration = Duration::from_secs(12);
/// How many catalogue lookups run at once. Both services answer small searches quickly, and
/// more sockets would only make a large import look busy without finishing sooner.
const LOOKUPS: usize = 4;
/// The most distinct albums one import will look up. A list past this still imports; the rows
/// it could not reach keep the honest placeholder and are counted.
pub const MAX_ALBUM_LOOKUPS: usize = 160;
/// The most distinct recordings one import will search for after the album pass.
pub const MAX_TRACK_LOOKUPS: usize = 80;
/// A cover larger than this is not needed for a collage tile and is not downloaded.
const MAX_COVER_BYTES: usize = 6 * 1024 * 1024;
const SEARCH_LIMIT: &str = "10";

/// What the enrichment pass changed, for the import result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Distinct albums and recordings that were looked up.
    pub looked_up: usize,
    /// Rows that gained a cover.
    pub covers: usize,
    /// Rows whose missing length was filled in.
    pub durations: usize,
    /// Lookups that returned nothing that matched.
    pub failed: usize,
    /// Albums or recordings left alone because the request cap was reached.
    pub skipped: usize,
}

impl Report {
    /// The number of rows the pass touched in any way.
    pub fn enriched(&self) -> usize {
        self.covers + self.durations
    }
}

/// What a second pass over an already saved imported playlist changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Refresh {
    /// Rows that gained a picture.
    pub covers: usize,
    /// Rows whose missing length was filled in.
    pub durations: usize,
    /// Whether the playlist's own cover was rebuilt from the new art.
    pub collage: bool,
    /// Rows that can still show a picture and have none.
    pub missing: usize,
}

impl Refresh {
    /// Whether the pass found anything at all.
    pub fn changed(&self) -> bool {
        self.covers + self.durations > 0
    }
}

/// The two public catalogues one import may ask, sharing one connection pool. A catalogue that
/// answers 403 or 429 is switched off for the rest of the import rather than asked again for
/// every row, so a blocked or throttled service costs one request instead of hundreds.
#[derive(Clone)]
pub struct Enricher {
    http: reqwest::Client,
    itunes: String,
    deezer: String,
    itunes_up: Arc<AtomicBool>,
    deezer_up: Arc<AtomicBool>,
}

impl Enricher {
    /// The client the importer uses: no account, no key, a short timeout and an honest name.
    pub fn public() -> Self {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("Soundy/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            http,
            itunes: ITUNES.to_owned(),
            deezer: DEEZER.to_owned(),
            itunes_up: Arc::new(AtomicBool::new(true)),
            deezer_up: Arc::new(AtomicBool::new(true)),
        }
    }

    /// The same client against two other origins, so a test can answer it without the network.
    pub fn at(itunes: impl Into<String>, deezer: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            http,
            itunes: itunes.into(),
            deezer: deezer.into(),
            itunes_up: Arc::new(AtomicBool::new(true)),
            deezer_up: Arc::new(AtomicBool::new(true)),
        }
    }

    /// One JSON answer, with the catalogue switched off when it is blocked, throttled or
    /// broken. A transient connection error leaves it on for the next group.
    async fn json<T: DeserializeOwned>(
        &self,
        server: &AtomicBool,
        request: reqwest::RequestBuilder,
    ) -> Option<T> {
        if !server.load(Ordering::Relaxed) {
            return None;
        }
        let response = request.send().await.ok()?;
        let status = response.status();
        if !status.is_success() {
            if matches!(status.as_u16(), 401 | 403 | 429) || status.is_server_error() {
                server.store(false, Ordering::Relaxed);
            }
            return None;
        }
        response.json().await.ok()
    }

    /// Fills the cover and any missing length of every entry it can prove. Cover URLs already
    /// in the export are kept, with an iTunes thumbnail widened to the largest it publishes.
    /// `cancel` is checked between batches; `progress` receives the number of groups finished
    /// and the number this phase intends to do.
    pub async fn enrich(
        &self,
        entries: &mut [SourceEntry],
        cancel: &AtomicBool,
        progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Report {
        let mut report = Report::default();
        let mut covered: HashSet<usize> = HashSet::new();
        for (position, entry) in entries.iter_mut().enumerate() {
            if let Some(cover) = entry.cover.take() {
                entry.cover = Some(large_cover(&cover));
                covered.insert(position);
            }
        }

        let albums = album_groups(entries, &covered);
        let album_total = albums.len().min(MAX_ALBUM_LOOKUPS);
        report.skipped += albums.len().saturating_sub(album_total);
        progress(0, album_total);

        let mut looked_up = 0;
        for group in albums.chunks(LOOKUPS) {
            if cancel.load(Ordering::Relaxed) || looked_up >= album_total {
                break;
            }
            let group = &group[..group.len().min(album_total - looked_up)];
            let answers = futures::future::join_all(
                group
                    .iter()
                    .map(|group| async { self.album_cover(&group.artist, &group.album).await }),
            )
            .await;
            for (group, cover) in group.iter().zip(answers) {
                looked_up += 1;
                match cover {
                    Some(url) => {
                        for &position in &group.positions {
                            if entries[position].cover.is_none() {
                                entries[position].cover = Some(url.clone());
                                covered.insert(position);
                                report.covers += 1;
                            }
                        }
                    }
                    None => report.failed += 1,
                }
            }
            progress(looked_up, album_total);
        }
        report.looked_up += looked_up;

        if cancel.load(Ordering::Relaxed) {
            return report;
        }

        let tracks = track_groups(entries, &covered);
        let track_total = tracks.len().min(MAX_TRACK_LOOKUPS);
        report.skipped += tracks.len().saturating_sub(track_total);
        progress(0, track_total);

        let mut looked_up = 0;
        for group in tracks.chunks(LOOKUPS) {
            if cancel.load(Ordering::Relaxed) || looked_up >= track_total {
                break;
            }
            let group = &group[..group.len().min(track_total - looked_up)];
            let answers = futures::future::join_all(group.iter().map(|group| async {
                self.track(&group.title, &group.artist, &group.album, group.duration)
                    .await
            }))
            .await;
            for (group, found) in group.iter().zip(answers) {
                looked_up += 1;
                match found {
                    Some(found) => {
                        for &position in &group.positions {
                            if let Some(cover) = found.cover.clone()
                                && entries[position].cover.is_none()
                            {
                                entries[position].cover = Some(cover);
                                covered.insert(position);
                                report.covers += 1;
                            }
                            if entries[position].duration.is_zero()
                                && let Some(duration) = found.duration
                            {
                                entries[position].duration = duration;
                                report.durations += 1;
                            }
                        }
                    }
                    None => report.failed += 1,
                }
            }
            progress(looked_up, track_total);
        }
        report.looked_up += looked_up;
        report
    }

    /// The cover of an album that the credited artist made, in either catalogue's largest size.
    async fn album_cover(&self, artist: &str, album: &str) -> Option<String> {
        let term = format!("{artist} {album}");
        if let Some(url) = self.deezer_album(&term, artist, album).await {
            return Some(url);
        }
        self.itunes_album(&term, artist, album).await
    }

    async fn deezer_album(&self, term: &str, artist: &str, album: &str) -> Option<String> {
        let page: DeezerAlbums = self
            .json(
                &self.deezer_up,
                self.http
                    .get(format!("{}/search/album", self.deezer))
                    .query(&[("q", term), ("limit", SEARCH_LIMIT)]),
            )
            .await?;
        page.data
            .into_iter()
            .find(|found| {
                album_key(&found.title) == album_key(album)
                    && same_primary(artist, &found.artist.name)
            })
            .and_then(|found| found.cover())
    }

    async fn itunes_album(&self, term: &str, artist: &str, album: &str) -> Option<String> {
        let page: ItunesAlbums = self
            .json(
                &self.itunes_up,
                self.http.get(format!("{}/search", self.itunes)).query(&[
                    ("term", term),
                    ("entity", "album"),
                    ("media", "music"),
                    ("limit", SEARCH_LIMIT),
                ]),
            )
            .await?;
        page.results
            .into_iter()
            .find(|found| {
                found
                    .collection_name
                    .as_deref()
                    .is_some_and(|name| album_key(name) == album_key(album))
                    && found
                        .artist_name
                        .as_deref()
                        .is_some_and(|name| same_primary(artist, name))
            })
            .and_then(|found| found.artwork_url_100)
            .map(|url| large_cover(&url))
    }

    /// One recording's cover and length, when a catalogue describes the same take.
    async fn track(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        duration: Duration,
    ) -> Option<Found> {
        let term = format!("{title} {artist}");
        if let Some(found) = self
            .deezer_track(&term, title, artist, album, duration)
            .await
        {
            return Some(found);
        }
        self.itunes_track(&term, title, artist, album, duration)
            .await
    }

    async fn deezer_track(
        &self,
        term: &str,
        title: &str,
        artist: &str,
        album: &str,
        duration: Duration,
    ) -> Option<Found> {
        let page: DeezerTracks = self
            .json(
                &self.deezer_up,
                self.http
                    .get(format!("{}/search", self.deezer))
                    .query(&[("q", term), ("limit", SEARCH_LIMIT)]),
            )
            .await?;
        page.data
            .into_iter()
            .find(|found| {
                accepts(
                    title,
                    artist,
                    album,
                    duration,
                    &found.title,
                    &found.artist.name,
                    found.album.as_ref().map(|album| album.title.as_str()),
                    Some(Duration::from_secs(found.duration)),
                )
            })
            .map(|found| Found {
                cover: found.album.and_then(|album| album.cover()),
                duration: Some(Duration::from_secs(found.duration))
                    .filter(|value| !value.is_zero()),
            })
    }

    async fn itunes_track(
        &self,
        term: &str,
        title: &str,
        artist: &str,
        album: &str,
        duration: Duration,
    ) -> Option<Found> {
        let page: ItunesTracks = self
            .json(
                &self.itunes_up,
                self.http.get(format!("{}/search", self.itunes)).query(&[
                    ("term", term),
                    ("entity", "song"),
                    ("media", "music"),
                    ("limit", SEARCH_LIMIT),
                ]),
            )
            .await?;
        page.results
            .into_iter()
            .find(|found| {
                let found_duration = found
                    .track_time_millis
                    .filter(|value| *value > 0)
                    .map(Duration::from_millis);
                accepts(
                    title,
                    artist,
                    album,
                    duration,
                    found.track_name.as_deref().unwrap_or_default(),
                    found.artist_name.as_deref().unwrap_or_default(),
                    found.collection_name.as_deref(),
                    found_duration,
                )
            })
            .map(|found| Found {
                cover: found.artwork_url_100.map(|url| large_cover(&url)),
                duration: found
                    .track_time_millis
                    .filter(|value| *value > 0)
                    .map(Duration::from_millis),
            })
    }

    /// Downloads one cover for the collage. The body is size-checked before it is kept, and a
    /// non-image or an oversized body is simply not a tile.
    pub async fn cover_bytes(&self, url: &str) -> Option<Vec<u8>> {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return None;
        }
        let response = self
            .http
            .get(url)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_COVER_BYTES as u64)
        {
            return None;
        }
        let bytes = response.bytes().await.ok()?;
        (bytes.len() <= MAX_COVER_BYTES).then(|| bytes.to_vec())
    }
}

/// A recording both catalogues agree on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Found {
    cover: Option<String>,
    duration: Option<Duration>,
}

/// The widest version of a catalogue thumbnail. iTunes serves the same picture at several
/// sizes under names such as `100x100bb.jpg`; the export usually carries the smallest.
pub fn large_cover(url: &str) -> String {
    for (small, large) in [
        ("100x100bb", "600x600bb"),
        ("60x60bb", "600x600bb"),
        ("30x30bb", "600x600bb"),
    ] {
        if url.contains(small) {
            return url.replace(small, large);
        }
    }
    url.to_owned()
}

/// One album's rows, so forty albums cost forty lookups.
struct Group {
    positions: Vec<usize>,
    artist: String,
    album: String,
    title: String,
    duration: Duration,
}

fn album_groups(entries: &[SourceEntry], covered: &HashSet<usize>) -> Vec<Group> {
    let mut groups: HashMap<(String, String), Group> = HashMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for (position, entry) in entries.iter().enumerate() {
        if covered.contains(&position)
            || entry.album.trim().is_empty()
            || entry.artists.trim().is_empty()
        {
            continue;
        }
        let group_key = (key(&entry.album), primary(&entry.artists));
        match groups.get_mut(&group_key) {
            Some(group) => group.positions.push(position),
            None => {
                order.push(group_key.clone());
                groups.insert(
                    group_key,
                    Group {
                        positions: vec![position],
                        artist: entry.artists.clone(),
                        album: entry.album.clone(),
                        title: entry.title.clone(),
                        duration: entry.duration,
                    },
                );
            }
        }
    }
    order
        .into_iter()
        .filter_map(|group_key| groups.remove(&group_key))
        .collect()
}

fn track_groups(entries: &[SourceEntry], covered: &HashSet<usize>) -> Vec<Group> {
    let mut groups: HashMap<(String, String, u64), Group> = HashMap::new();
    let mut order: Vec<(String, String, u64)> = Vec::new();
    for (position, entry) in entries.iter().enumerate() {
        if entry.artists.trim().is_empty() {
            continue;
        }
        // A row that already has a picture is only worth a lookup when its length is missing.
        if covered.contains(&position) && !entry.duration.is_zero() {
            continue;
        }
        // Nothing proves an untimed, album-less row is the studio take rather than a cover.
        if entry.duration.is_zero() && entry.album.trim().is_empty() {
            continue;
        }
        let group_key = (
            key(&entry.title),
            primary(&entry.artists),
            entry.duration.as_secs(),
        );
        match groups.get_mut(&group_key) {
            Some(group) => group.positions.push(position),
            None => {
                order.push(group_key.clone());
                groups.insert(
                    group_key,
                    Group {
                        positions: vec![position],
                        artist: entry.artists.clone(),
                        album: entry.album.clone(),
                        title: entry.title.clone(),
                        duration: entry.duration,
                    },
                );
            }
        }
    }
    order
        .into_iter()
        .filter_map(|group_key| groups.remove(&group_key))
        .collect()
}

/// The leading credited name, normalized. Both sides of a comparison are cut the same way, so
/// a provider that lists every guest still agrees on the first artist.
fn primary(artists: &str) -> String {
    artists
        .split([',', '&', ';'])
        .flat_map(|part| part.split(" and "))
        .map(key)
        .find(|name| !name.is_empty())
        .unwrap_or_default()
}

fn same_primary(wanted: &str, found: &str) -> bool {
    let wanted = primary(wanted);
    !wanted.is_empty() && primary(found) == wanted
}

/// An album name without the release-type suffix the catalogues append. iTunes calls a single's
/// album `Hardwired - Single` while an export writes `Hardwired`; the two are the same release.
/// A title that is only the suffix (`Single`) keeps its name rather than becoming empty.
fn album_key(text: &str) -> String {
    let key = key(text);
    for suffix in [" single", " ep", " lp"] {
        if let Some(rest) = key.strip_suffix(suffix)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    key
}

/// Whether a catalogue answer describes the very take the row names. The title key keeps
/// version words, so a remix or live version never matches a studio title.
// Eight arguments keep the row and the answer side by side at each call site; grouping them
// into a struct would hide which half a value belongs to.
#[allow(clippy::too_many_arguments)]
fn accepts(
    title: &str,
    artist: &str,
    album: &str,
    duration: Duration,
    found_title: &str,
    found_artist: &str,
    found_album: Option<&str>,
    found_duration: Option<Duration>,
) -> bool {
    if key(found_title) != key(title) || !same_primary(artist, found_artist) {
        return false;
    }
    match (
        duration.is_zero(),
        found_duration.filter(|value| !value.is_zero()),
    ) {
        (false, Some(found)) => duration.abs_diff(found) <= SAME_RECORDING,
        // A catalogue answer with no length cannot prove it is the same take.
        (false, None) => false,
        (true, _) => {
            !album.trim().is_empty()
                && found_album.is_some_and(|found| album_key(found) == album_key(album))
        }
    }
}

//
// Wire types. Only the fields an import reads are named; serde ignores the rest.
//

#[derive(Deserialize)]
struct DeezerAlbums {
    #[serde(default)]
    data: Vec<DeezerAlbum>,
}

#[derive(Deserialize, Default)]
struct DeezerAlbum {
    title: String,
    #[serde(default)]
    artist: DeezerArtist,
    #[serde(default)]
    cover_xl: Option<String>,
    #[serde(default)]
    cover_big: Option<String>,
    #[serde(default)]
    cover_medium: Option<String>,
}

impl DeezerAlbum {
    fn cover(self) -> Option<String> {
        self.cover_xl.or(self.cover_big).or(self.cover_medium)
    }
}

#[derive(Deserialize, Default)]
struct DeezerArtist {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct DeezerTracks {
    #[serde(default)]
    data: Vec<DeezerTrack>,
}

#[derive(Deserialize)]
struct DeezerTrack {
    title: String,
    #[serde(default)]
    duration: u64,
    artist: DeezerArtist,
    #[serde(default)]
    album: Option<DeezerAlbum>,
}

#[derive(Deserialize)]
struct ItunesAlbums {
    #[serde(default)]
    #[serde(rename = "results")]
    results: Vec<ItunesAlbum>,
}

#[derive(Deserialize)]
struct ItunesAlbum {
    #[serde(default, rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(default, rename = "artistName")]
    artist_name: Option<String>,
    #[serde(default, rename = "artworkUrl100")]
    artwork_url_100: Option<String>,
}

#[derive(Deserialize)]
struct ItunesTracks {
    #[serde(default, rename = "results")]
    results: Vec<ItunesTrack>,
}

#[derive(Deserialize)]
struct ItunesTrack {
    #[serde(default, rename = "trackName")]
    track_name: Option<String>,
    #[serde(default, rename = "artistName")]
    artist_name: Option<String>,
    #[serde(default, rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(default, rename = "trackTimeMillis")]
    track_time_millis: Option<u64>,
    #[serde(default, rename = "artworkUrl100")]
    artwork_url_100: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;

    fn entry(title: &str, artist: &str, album: &str, seconds: u64) -> SourceEntry {
        SourceEntry {
            title: title.into(),
            artists: artist.into(),
            album: album.into(),
            duration: Duration::from_secs(seconds),
            ..Default::default()
        }
    }

    /// A one-answer HTTP server, so each lookup is exercised as a real request.
    fn serve(body: &str) -> String {
        serve_many(body, 1)
    }

    /// The same answer for `answers` requests, for a row that needs more than one catalogue.
    fn serve_many(body: &str, answers: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        std::thread::spawn(move || {
            for _ in 0..answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{address}")
    }

    const NO_SERVER: &str = "http://127.0.0.1:1";

    #[test]
    fn widens_an_itunes_thumbnail() {
        assert_eq!(
            large_cover("https://is1-ssl.mzstatic.com/image/thumb/a/100x100bb.jpg"),
            "https://is1-ssl.mzstatic.com/image/thumb/a/600x600bb.jpg"
        );
        assert_eq!(
            large_cover("https://cdn-images.dzcdn.net/images/cover/a/1000x1000.jpg"),
            "https://cdn-images.dzcdn.net/images/cover/a/1000x1000.jpg"
        );
    }

    #[test]
    fn an_itunes_release_suffix_does_not_hide_the_album() {
        assert_eq!(album_key("Hardwired - Single"), album_key("Hardwired"));
        assert_eq!(album_key("Bloom - EP"), album_key("Bloom"));
        // A real title that is only the release word is not erased.
        assert_eq!(album_key("Single"), "single");
    }

    #[test]
    fn never_accepts_a_remix_cover_or_untimed_stranger() {
        let duration = Duration::from_secs(302);
        assert!(accepts(
            "Hardwired",
            "Rameses B",
            "Hardwired",
            duration,
            "Hardwired",
            "Rameses B",
            Some("Hardwired"),
            Some(Duration::from_secs(300))
        ));
        assert!(!accepts(
            "Hardwired",
            "Rameses B",
            "Hardwired",
            duration,
            "Hardwired (Remix)",
            "Rameses B",
            Some("Hardwired"),
            Some(Duration::from_secs(302))
        ));
        assert!(!accepts(
            "Hardwired",
            "Rameses B",
            "Hardwired",
            duration,
            "Hardwired",
            "Cover Band",
            Some("Hardwired"),
            Some(Duration::from_secs(302))
        ));
        assert!(!accepts(
            "Hardwired",
            "Rameses B",
            "Hardwired",
            duration,
            "Hardwired",
            "Rameses B",
            Some("Hardwired"),
            None
        ));
        // With no source length the album is the proof.
        assert!(accepts(
            "Hardwired",
            "Rameses B",
            "Hardwired",
            Duration::ZERO,
            "Hardwired",
            "Rameses B",
            Some("Hardwired"),
            Some(Duration::from_secs(302))
        ));
        assert!(!accepts(
            "Hardwired",
            "Rameses B",
            "Hardwired",
            Duration::ZERO,
            "Hardwired",
            "Rameses B",
            Some("Another Album"),
            Some(Duration::from_secs(302))
        ));
    }

    #[tokio::test]
    async fn one_album_lookup_covers_every_row_on_that_album() {
        let body = r#"{"data":[{"title":"Hardwired","artist":{"name":"Rameses B"},"cover_xl":"https://cdn.example/hardwired.jpg"}]}"#;
        let enricher = Enricher::at(NO_SERVER, serve(body));
        let mut entries = vec![
            entry("Hardwired", "Rameses B", "Hardwired", 302),
            entry("Neon", "Rameses B", "Hardwired", 240),
        ];
        let report = enricher
            .enrich(&mut entries, &AtomicBool::new(false), &|_, _| {})
            .await;
        assert_eq!(report.covers, 2);
        assert_eq!(report.looked_up, 1);
        assert_eq!(
            entries[0].cover.as_deref(),
            Some("https://cdn.example/hardwired.jpg")
        );
        assert_eq!(entries[1].cover, entries[0].cover);
    }

    #[tokio::test]
    async fn a_track_lookup_fills_the_cover_and_the_missing_length() {
        let body = r#"{"data":[{"title":"Hardwired","duration":302,"artist":{"name":"Rameses B"},"album":{"title":"Hardwired","cover_big":"https://cdn.example/hardwired.jpg"}}]}"#;
        // The album search reads this answer too, finds no picture in it, and leaves the row to
        // the recording search, which fills both the cover and the length.
        let enricher = Enricher::at(NO_SERVER, serve_many(body, 2));
        let mut entries = vec![entry("Hardwired", "Rameses B", "Hardwired", 0)];
        let report = enricher
            .enrich(&mut entries, &AtomicBool::new(false), &|_, _| {})
            .await;
        assert_eq!(report.covers, 1);
        assert_eq!(report.durations, 1);
        assert_eq!(entries[0].duration, Duration::from_secs(302));
        assert_eq!(
            entries[0].cover.as_deref(),
            Some("https://cdn.example/hardwired.jpg")
        );
    }

    #[tokio::test]
    async fn falls_back_to_itunes_and_widens_its_artwork() {
        let body = r#"{"results":[{"collectionName":"Hardwired","artistName":"Rameses B","artworkUrl100":"https://is1.example/a/100x100bb.jpg"}]}"#;
        let enricher = Enricher::at(serve(body), NO_SERVER);
        let mut entries = vec![entry("Hardwired", "Rameses B", "Hardwired", 302)];
        let report = enricher
            .enrich(&mut entries, &AtomicBool::new(false), &|_, _| {})
            .await;
        assert_eq!(report.covers, 1);
        assert_eq!(
            entries[0].cover.as_deref(),
            Some("https://is1.example/a/600x600bb.jpg")
        );
    }

    /// The wire shape and the live service, run by hand with `--ignored`. Deezer blocks some
    /// networks, so this exercises the iTunes half, which needs no key either.
    #[tokio::test]
    #[ignore = "asks the live iTunes Search API"]
    async fn finds_and_downloads_real_itunes_artwork() {
        let enricher = Enricher::public();
        let cover = enricher
            .itunes_album("Rameses B Hardwired", "Rameses B", "Hardwired")
            .await
            .expect("iTunes knows this album");
        assert!(!cover.contains("100x100bb"), "{cover}");
        let bytes = enricher
            .cover_bytes(&cover)
            .await
            .expect("the cover downloads");
        assert!(bytes.len() > 10_000, "{} bytes", bytes.len());
        image::load_from_memory(&bytes).expect("the cover is an image");
    }
}
