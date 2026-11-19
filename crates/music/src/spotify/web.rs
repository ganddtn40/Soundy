//! The Spotify Web API calls the importer needs.
//!
//! Only two endpoints are read: the playlist itself, which carries its first page of entries,
//! and `/items` for the pages after it. A bearer token comes either from a signed-in librespot
//! session or from the importer's own metadata-only sign-in; Soundy ships no developer secret
//! and never reads a stream URL from here. A 401 asks for the token once more, a 429 waits the
//! `Retry-After` Spotify names, and a 403 is reported as the authorization restriction it is
//! rather than worked around.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context as _;
use async_trait::async_trait;
use librespot_core::Session;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::time::sleep;

use crate::import::{ImportFailure, SourceEntry, SourcePlaylist};

const BASE: &str = "https://api.spotify.com/v1";
/// The most a playlist-items page returns. Spotify's current maximum is 50; a larger `limit`
/// is rejected rather than clamped.
const PAGE: usize = 50;
/// How long one request may take before it is treated as a network failure.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How many 429 answers in a row are waited out before the import gives up.
const RETRIES: usize = 3;
/// The longest `Retry-After` honoured. A longer one is reported rather than waited silently.
const LONGEST_WAIT: u64 = 30;
/// A cover bigger than this is not a playlist cover and is refused unread.
const IMAGE_LIMIT: usize = 4 * 1024 * 1024;

/// Where a Web API call gets its bearer token. The real one is the signed-in librespot
/// session; tests answer with a fixed string.
#[async_trait]
pub trait TokenSource: Send + Sync {
    async fn authorization(&self) -> anyhow::Result<String>;
}

struct Login5(Session);

#[async_trait]
impl TokenSource for Login5 {
    async fn authorization(&self) -> anyhow::Result<String> {
        let token = self
            .0
            .login5()
            .auth_token()
            .await
            .context("cannot obtain a Spotify access token")?;
        Ok(format!("{} {}", token.token_type, token.access_token))
    }
}

/// A token obtained once by the importer, for a build that has no playback session. It lasts an
/// hour, which is longer than any playlist import, so it is not refreshed mid-flight.
struct Bearer(String);

#[async_trait]
impl TokenSource for Bearer {
    async fn authorization(&self) -> anyhow::Result<String> {
        Ok(format!("Bearer {}", self.0))
    }
}

/// Reads playlists from Spotify's public Web API with the session's own token.
#[derive(Clone)]
pub struct Web {
    http: reqwest::Client,
    token: Arc<dyn TokenSource>,
    base: String,
}

impl Web {
    pub fn new(session: Session) -> Self {
        Self {
            http: client(),
            token: Arc::new(Login5(session)),
            base: BASE.to_owned(),
        }
    }

    /// Reads with a bearer token another part of the app obtained, such as the importer's
    /// metadata-only sign-in. The token is used as given and never persisted here.
    pub fn with_bearer(token: impl Into<String>) -> Self {
        Self {
            http: client(),
            token: Arc::new(Bearer(token.into())),
            base: BASE.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn at(base: impl Into<String>, token: impl TokenSource + 'static) -> Self {
        Self {
            http: client(),
            token: Arc::new(token),
            base: base.into(),
        }
    }

    /// Reads the playlist and every entry, page by page. `progress` is handed the number of
    /// entries read and the total Spotify reported, and `cancel` is checked between requests
    /// and while a rate-limit wait runs.
    pub async fn playlist(
        &self,
        id: &str,
        cancel: &AtomicBool,
        progress: &(dyn Fn(usize, usize) + Send + Sync),
    ) -> Result<SourcePlaylist, ImportFailure> {
        let url = format!("{}/playlists/{}", self.base, id);
        let first: Body = self.get(&url, cancel).await?;

        let mut playlist = first.into_playlist(id);
        let mut seen = playlist.entries.len();
        progress(seen, playlist.total);

        let mut offset = seen;
        while offset < playlist.total && seen < playlist.total {
            if cancel.load(Ordering::Relaxed) {
                return Err(ImportFailure::Cancelled);
            }
            let url = format!(
                "{}/playlists/{}/items?limit={PAGE}&offset={offset}&additional_types=track",
                self.base, id
            );
            let body: Page = self.get(&url, cancel).await?;
            let page: Vec<SourceEntry> = body.items.into_iter().map(entry).collect();
            if page.is_empty() {
                break;
            }
            seen += page.len();
            offset += page.len();
            playlist.entries.extend(page);
            if body.next.is_none() {
                break;
            }
            progress(seen, playlist.total.max(seen));
        }

        playlist.total = playlist.total.max(playlist.entries.len());
        progress(playlist.entries.len(), playlist.total);
        Ok(playlist)
    }

    /// Downloads a cover. The caller decides whether to write it; nothing is decoded here, so
    /// the source artwork is kept byte for byte.
    pub async fn image(&self, url: &str) -> Result<Vec<u8>, ImportFailure> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| ImportFailure::Network(error.to_string()))?;
        if !response.status().is_success() {
            return Err(ImportFailure::Other(format!(
                "Spotify's image host answered {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length as usize > IMAGE_LIMIT)
        {
            return Err(ImportFailure::Other("the cover is too large".to_owned()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ImportFailure::Network(error.to_string()))?;
        if bytes.len() > IMAGE_LIMIT {
            return Err(ImportFailure::Other("the cover is too large".to_owned()));
        }
        Ok(bytes.to_vec())
    }

    async fn get<T: DeserializeOwned>(
        &self,
        url: &str,
        cancel: &AtomicBool,
    ) -> Result<T, ImportFailure> {
        let mut refreshed = false;
        let mut waits = 0_usize;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ImportFailure::Cancelled);
            }
            let authorization = self
                .token
                .authorization()
                .await
                .map_err(|error| ImportFailure::Other(format!("{error:#}")))?;
            let response = self
                .http
                .get(url)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(reqwest::header::AUTHORIZATION, authorization)
                .send()
                .await
                .map_err(|error| ImportFailure::Network(error.to_string()))?;
            let status = response.status();
            match status.as_u16() {
                200..=299 => {
                    let bytes = response
                        .bytes()
                        .await
                        .map_err(|error| ImportFailure::Network(error.to_string()))?;
                    return serde_json::from_slice(&bytes).map_err(|error| {
                        ImportFailure::Other(format!("cannot read Spotify's answer: {error}"))
                    });
                }
                401 if !refreshed => {
                    // The token handed out may have expired mid-import. Ask librespot once for
                    // the session's current one, then try the same request again.
                    refreshed = true;
                }
                401 => return Err(ImportFailure::Unauthorized),
                403 => return Err(ImportFailure::Forbidden),
                404 => return Err(ImportFailure::NotFound),
                429 => {
                    let seconds = retry_after(&response);
                    if waits >= RETRIES || seconds > LONGEST_WAIT {
                        return Err(ImportFailure::RateLimited { seconds });
                    }
                    waits += 1;
                    let mut left = seconds;
                    while left > 0 {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(ImportFailure::Cancelled);
                        }
                        sleep(Duration::from_secs(1)).await;
                        left -= 1;
                    }
                }
                _ => {
                    return Err(ImportFailure::Other(format!("Spotify answered {status}")));
                }
            }
        }
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("Soundy/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("a plain HTTPS client")
}

fn retry_after(response: &reqwest::Response) -> u64 {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(1)
}

#[derive(Deserialize)]
struct Body {
    id: Option<String>,
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    owner: Option<Owner>,
    #[serde(default)]
    images: Vec<Image>,
    external_urls: Option<External>,
    /// Spotify moved the playlist's paging part from `tracks` to `items`. Both are the same
    /// shape, and the deprecated name is still accepted so an older answer reads too.
    #[serde(default)]
    tracks: Option<Page>,
    #[serde(default)]
    items: Option<Page>,
}

impl Body {
    fn into_playlist(self, asked: &str) -> SourcePlaylist {
        let id = self.id.unwrap_or_else(|| asked.to_owned());
        let (items, total) = match self.items.or(self.tracks) {
            Some(page) => {
                let total = page.total.max(page.offset + page.items.len());
                (page.items, total)
            }
            None => (Vec::new(), 0),
        };
        SourcePlaylist {
            source: "spotify".to_owned(),
            id,
            name: self
                .name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Spotify playlist".to_owned()),
            description: self.description.unwrap_or_default(),
            owner: self
                .owner
                .and_then(|owner| owner.display_name.or(owner.id))
                .unwrap_or_default(),
            cover: self.images.into_iter().next().map(|image| image.url),
            source_url: self
                .external_urls
                .and_then(|urls| urls.spotify)
                .unwrap_or_else(|| format!("https://open.spotify.com/playlist/{asked}")),
            total,
            skipped: 0,
            entries: items.into_iter().map(entry).collect(),
        }
    }
}

#[derive(Deserialize)]
struct Owner {
    id: Option<String>,
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct Image {
    url: String,
}

#[derive(Deserialize)]
struct External {
    spotify: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    total: usize,
    next: Option<String>,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    /// The current name for the entry's track or episode; `track` is the deprecated earlier
    /// name and is kept as a fallback.
    item: Option<ItemTrack>,
    #[serde(default)]
    track: Option<ItemTrack>,
    #[serde(default)]
    is_local: bool,
    added_at: Option<String>,
}

#[derive(Deserialize)]
struct ItemTrack {
    id: Option<String>,
    name: Option<String>,
    #[serde(default)]
    artists: Vec<Named>,
    album: Option<ItemAlbum>,
    duration_ms: Option<u64>,
    external_ids: Option<ExternalIds>,
    external_urls: Option<External>,
    is_playable: Option<bool>,
    preview_url: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    is_local: bool,
    #[serde(default)]
    show: Option<Named>,
    #[serde(default)]
    images: Vec<Image>,
}

#[derive(Deserialize)]
struct Named {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ItemAlbum {
    name: Option<String>,
    #[serde(default)]
    images: Vec<Image>,
}

#[derive(Deserialize)]
struct ExternalIds {
    isrc: Option<String>,
}

fn entry(item: Item) -> SourceEntry {
    let local = item.is_local;
    let Some(track) = item.item.or(item.track) else {
        return SourceEntry {
            title: "Unavailable track".to_owned(),
            local,
            added_at: crate::iso_8601_to_epoch(item.added_at.as_deref()),
            ..Default::default()
        };
    };
    let podcast = track.kind.as_deref() == Some("episode");
    let artists = if podcast {
        track
            .show
            .as_ref()
            .and_then(|show| show.name.clone())
            .unwrap_or_default()
    } else {
        track
            .artists
            .iter()
            .filter_map(|artist| artist.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (album, cover) = match (podcast, track.album) {
        (true, _) => (
            String::new(),
            track.images.into_iter().next().map(|image| image.url),
        ),
        (false, Some(album)) => (
            album.name.unwrap_or_default(),
            album.images.into_iter().next().map(|image| image.url),
        ),
        (false, None) => (String::new(), None),
    };
    let playable = !podcast
        && !local
        && !track.is_local
        && track.is_playable.unwrap_or(true)
        && track.id.is_some();
    SourceEntry {
        id: track.id,
        title: track.name.unwrap_or_else(|| "Unavailable track".to_owned()),
        artists,
        album,
        duration: Duration::from_millis(track.duration_ms.unwrap_or_default()),
        cover,
        isrc: track.external_ids.and_then(|ids| ids.isrc),
        release_year: None,
        url: track.external_urls.and_then(|urls| urls.spotify),
        local_path: None,
        playable,
        preview: track.preview_url.is_some(),
        local: local || track.is_local,
        added_at: crate::iso_8601_to_epoch(item.added_at.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    struct Fixed;
    #[async_trait]
    impl TokenSource for Fixed {
        async fn authorization(&self) -> anyhow::Result<String> {
            Ok("Bearer test".to_owned())
        }
    }

    struct Expiring(AtomicUsize);
    #[async_trait]
    impl TokenSource for Expiring {
        async fn authorization(&self) -> anyhow::Result<String> {
            let count = self.0.fetch_add(1, Ordering::Relaxed);
            Ok(match count {
                0 => "Bearer stale".to_owned(),
                _ => "Bearer fresh".to_owned(),
            })
        }
    }

    /// A tiny HTTP server that answers each request with the next response in `answers`.
    fn serve(answers: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for (status, body) in answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0_u8; 4096];
                let read = stream.read(&mut request).unwrap_or(0);
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&request[..read]).to_string());
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    403 => "Forbidden",
                    429 => "Too Many Requests",
                    _ => "Not Found",
                };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (format!("http://{address}"), seen)
    }

    fn item(id: &str, name: &str) -> String {
        track_field("track", id, name)
    }

    /// The entry shape Spotify documents now: the track lives under `item`.
    fn current_item(id: &str, name: &str) -> String {
        track_field("item", id, name)
    }

    fn track_field(field: &str, id: &str, name: &str) -> String {
        format!(
            r#"{{"{field}":{{"id":"{id}","name":"{name}","artists":[{{"name":"Artist"}}],"album":{{"name":"Album","images":[{{"url":"https://i.scdn.co/image/cover"}}]}},"duration_ms":201000,"external_ids":{{"isrc":"USABC1234567"}},"external_urls":{{"spotify":"https://open.spotify.com/track/{id}"}},"is_playable":true}},"added_at":"2024-01-02T03:04:05Z"}}"#
        )
    }

    fn page(items: &[String], offset: usize, total: usize, next: bool) -> String {
        format!(
            r#"{{"total":{total},"offset":{offset},"next":{},"items":[{}]}}"#,
            if next {
                "\"https://api.spotify.com/next\""
            } else {
                "null"
            },
            items.join(",")
        )
    }

    fn playlist(name: &str, first: String, total: usize) -> String {
        format!(
            r#"{{"id":"list","name":"{name}","description":"About","owner":{{"display_name":"Owner"}},"images":[{{"url":"https://i.scdn.co/image/list"}}],"external_urls":{{"spotify":"https://open.spotify.com/playlist/list"}},"tracks":{first},"total":{total}}}"#
        )
    }

    /// The current playlist object: the paging part is called `items`, not `tracks`.
    fn current_playlist(name: &str, first: String, total: usize) -> String {
        format!(
            r#"{{"id":"list","name":"{name}","description":"About","owner":{{"display_name":"Owner"}},"images":[{{"url":"https://i.scdn.co/image/list"}}],"external_urls":{{"spotify":"https://open.spotify.com/playlist/list"}},"items":{first},"total":{total}}}"#
        )
    }

    #[tokio::test]
    async fn reads_the_current_playlist_and_item_names() {
        let first = page(&[current_item("a", "One")], 0, 2, true);
        let rest = page(&[current_item("b", "Two")], 1, 2, false);
        let (base, seen) = serve(vec![(200, current_playlist("List", first, 2)), (200, rest)]);
        let web = Web::at(base, Fixed);
        let playlist = web
            .playlist("list", &AtomicBool::new(false), &|_, _| {})
            .await
            .unwrap();
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.entries[0].title, "One");
        assert_eq!(playlist.entries[1].title, "Two");
        assert_eq!(playlist.entries[1].id.as_deref(), Some("b"));
        let seen = seen.lock().unwrap();
        assert!(seen[1].contains("/playlists/list/items?limit=50&offset=1"));
    }

    #[tokio::test]
    async fn reads_the_first_page_from_the_playlist_and_the_rest_from_items() {
        let first = page(&[item("a", "One")], 0, 2, true);
        let rest = page(&[item("b", "Two")], 1, 2, false);
        let (base, seen) = serve(vec![(200, playlist("List", first, 2)), (200, rest)]);
        let web = Web::at(base, Fixed);
        let progress = Arc::new(Mutex::new(Vec::new()));
        let log = progress.clone();
        let playlist = web
            .playlist("list", &AtomicBool::new(false), &move |read, total| {
                log.lock().unwrap().push((read, total));
            })
            .await
            .unwrap();
        assert_eq!(playlist.name, "List");
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.entries[1].title, "Two");
        assert_eq!(playlist.entries[0].isrc.as_deref(), Some("USABC1234567"));
        let progress = progress.lock().unwrap();
        assert!(progress.contains(&(1, 2)), "{progress:?}");
        assert!(progress.contains(&(2, 2)), "{progress:?}");
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("/playlists/list "));
        assert!(seen[1].contains("/playlists/list/items?limit=50&offset=1"));
    }

    #[tokio::test]
    async fn asks_for_a_fresh_token_once_after_a_401() {
        let first = page(&[item("a", "One")], 0, 1, false);
        let (base, seen) = serve(vec![
            (401, "{}".to_owned()),
            (200, playlist("List", first, 1)),
        ]);
        let web = Web::at(base, Expiring(AtomicUsize::new(0)));
        let playlist = web
            .playlist("list", &AtomicBool::new(false), &|_, _| {})
            .await
            .unwrap();
        assert_eq!(playlist.entries.len(), 1);
        let seen = seen.lock().unwrap();
        assert!(seen[0].contains("Bearer stale"));
        assert!(seen[1].contains("Bearer fresh"));
    }

    #[tokio::test]
    async fn explains_a_403_without_trying_again() {
        let (base, _) = serve(vec![(403, "{}".to_owned())]);
        let web = Web::at(base, Fixed);
        let error = web
            .playlist("list", &AtomicBool::new(false), &|_, _| {})
            .await
            .unwrap_err();
        assert_eq!(error, ImportFailure::Forbidden);
    }

    #[tokio::test]
    async fn waits_out_a_429() {
        let first = page(&[item("a", "One")], 0, 1, false);
        let (base, seen) = serve(vec![
            (429, "{}".to_owned()),
            (200, playlist("List", first, 1)),
        ]);
        let web = Web::at(base, Fixed);
        let playlist = web
            .playlist("list", &AtomicBool::new(false), &|_, _| {})
            .await
            .unwrap();
        assert_eq!(playlist.entries.len(), 1);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn stops_when_cancelled() {
        let first = page(&[item("a", "One")], 0, 10, true);
        let (base, _) = serve(vec![(200, playlist("List", first, 10))]);
        let web = Web::at(base, Fixed);
        let error = web
            .playlist("list", &AtomicBool::new(true), &|_, _| {})
            .await
            .unwrap_err();
        assert_eq!(error, ImportFailure::Cancelled);
    }
}
