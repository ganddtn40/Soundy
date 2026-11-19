//! The playlist Spotify publishes on its public embed page.
//!
//! This is the importer's anonymous path: `open.spotify.com/embed/playlist/<id>` is the widget
//! Spotify serves to any browser with no account, and its `__NEXT_DATA__` payload names the
//! playlist and lists the tracks the widget shows. Nothing here signs in, reads a cookie, asks
//! for a private endpoint or copies audio; a playlist Spotify does not publish is reported as
//! unavailable instead.
//!
//! The widget publishes at most [`EMBED_LIMIT`] rows and exposes no reliable total, so a page
//! that comes back full is marked [`SourcePlaylist::limited`] rather than presented as complete.
//! Rows carry a title, artists, duration, Spotify id and playability; album names and per-track
//! artwork come from the public catalogue enrichment pass.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::import::{ImportFailure, SourceEntry, SourcePlaylist};

const EMBED: &str = "https://open.spotify.com/embed/playlist";
const PAGE: &str = "https://open.spotify.com/playlist";
/// The most rows the embed widget publishes. A page this long may have been cut short.
pub const EMBED_LIMIT: usize = 100;
const TIMEOUT: Duration = Duration::from_secs(20);
/// Attempts for a transient failure, including the first request.
const ATTEMPTS: usize = 3;
/// A `Retry-After` longer than this is reported instead of waited out, so a stuck import never
/// looks like a frozen window.
const MAX_WAIT: u64 = 20;
/// The honest client name. Spotify's public embed page serves it without an account.
const USER_AGENT: &str = concat!("Soundy/", env!("CARGO_PKG_VERSION"));

/// Reads the public playlist page for `id`.
///
/// A missing or private playlist is [`ImportFailure::NotFound`]; a page that returns no track
/// list is [`ImportFailure::TracklistUnavailable`]; a full page is kept and flagged as limited.
pub async fn playlist(id: &str, cancel: &AtomicBool) -> Result<SourcePlaylist, ImportFailure> {
    let http = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| ImportFailure::Network(error.to_string()))?;
    let url = format!("{EMBED}/{id}");

    let mut attempt = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ImportFailure::Cancelled);
        }
        let response = match http.get(&url).send().await {
            Ok(response) => response,
            Err(error) => {
                if attempt + 1 >= ATTEMPTS {
                    return Err(ImportFailure::Network(error.to_string()));
                }
                attempt += 1;
                backoff(attempt, cancel).await?;
                continue;
            }
        };
        let status = response.status();

        if status.is_success() {
            let body = match response_text(response, cancel).await {
                Ok(body) => body,
                Err(failure) => return Err(failure),
            };
            return parse(id, &body);
        }

        match status.as_u16() {
            404 => return Err(ImportFailure::NotFound),
            401 | 403 => return Err(ImportFailure::Forbidden),
            429 => {
                let seconds = retry_after(response, attempt);
                if attempt + 1 >= ATTEMPTS || seconds > MAX_WAIT {
                    return Err(ImportFailure::RateLimited { seconds });
                }
                attempt += 1;
                wait(seconds, cancel).await?;
            }
            code if code >= 500 => {
                if attempt + 1 >= ATTEMPTS {
                    return Err(ImportFailure::Network(format!("Spotify answered {code}")));
                }
                attempt += 1;
                backoff(attempt, cancel).await?;
            }
            code => {
                return Err(ImportFailure::Network(format!("Spotify answered {code}")));
            }
        }
    }
}

async fn response_text(
    response: reqwest::Response,
    cancel: &AtomicBool,
) -> Result<String, ImportFailure> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ImportFailure::Cancelled);
    }
    response
        .text()
        .await
        .map_err(|error| ImportFailure::Network(error.to_string()))
}

/// Reads a `Retry-After` in seconds, falling back to an exponential wait.
fn retry_after(response: reqwest::Response, attempt: usize) -> u64 {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or_else(|| 1_u64 << attempt.min(4))
}

/// Waits between transient retries, in small steps, so cancellation is noticed promptly.
async fn backoff(attempt: usize, cancel: &AtomicBool) -> Result<(), ImportFailure> {
    wait(1_u64 << (attempt - 1).min(3), cancel).await
}

async fn wait(seconds: u64, cancel: &AtomicBool) -> Result<(), ImportFailure> {
    let deadline = Instant::now() + Duration::from_secs(seconds.min(MAX_WAIT));
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return Err(ImportFailure::Cancelled);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

/// Parses the public page. Split from the request so the payload can be tested without a
/// network and a changed page reports a page failure rather than a fabricated playlist.
pub fn parse(id: &str, html: &str) -> Result<SourcePlaylist, ImportFailure> {
    let payload = next_data(html).ok_or(ImportFailure::PublicPageChanged)?;
    let data: NextData =
        serde_json::from_str(payload).map_err(|_| ImportFailure::PublicPageChanged)?;

    if data.props.page_props.status == Some(404) {
        return Err(ImportFailure::NotFound);
    }
    let Some(state) = data.props.page_props.state else {
        return Err(ImportFailure::PublicPageChanged);
    };
    let Some(entity) = state.data.entity else {
        return Err(ImportFailure::NotFound);
    };
    if entity.kind != "playlist" {
        return Err(ImportFailure::TracklistUnavailable);
    }
    if entity.track_list.is_empty() {
        return match entity.is_playable {
            Some(false) => Err(ImportFailure::TracklistUnavailable),
            _ => Err(ImportFailure::EmptySource),
        };
    }

    let mut entries = Vec::with_capacity(entity.track_list.len());
    let mut skipped = 0;
    for row in entity.track_list {
        let title = row
            .title
            .unwrap_or_default()
            .replace('\u{00A0}', " ")
            .trim()
            .to_owned();
        if title.is_empty() {
            skipped += 1;
            continue;
        }
        let track_id = row
            .uri
            .strip_prefix("spotify:track:")
            .map(str::to_owned)
            .filter(|id| !id.is_empty());
        entries.push(SourceEntry {
            id: track_id.clone(),
            title,
            artists: artists(&row.subtitle),
            duration: Duration::from_millis(row.duration.unwrap_or_default()),
            url: track_id.map(|id| format!("https://open.spotify.com/track/{id}")),
            playable: row.is_playable,
            preview: !row.is_playable && row.audio_preview.is_some(),
            ..Default::default()
        });
    }
    if entries.is_empty() {
        return Err(ImportFailure::EmptySource);
    }

    let limited = entries.len() >= EMBED_LIMIT;
    Ok(SourcePlaylist {
        source: "spotify".to_owned(),
        id: id.to_owned(),
        name: match entity.name.filter(|name| !name.trim().is_empty()) {
            Some(name) => name,
            None => entity.title,
        },
        description: String::new(),
        owner: entity.subtitle.replace('\u{00A0}', " ").trim().to_owned(),
        cover: entity
            .cover_art
            .and_then(|art| art.sources.into_iter().next())
            .map(|image| image.url),
        source_url: format!("{PAGE}/{id}"),
        total: entries.len() + skipped,
        skipped,
        entries,
        limited,
    })
}

/// The playlist's rows credit artists with a comma and a non-breaking space. Both separators
/// are folded back to a plain `, ` so matching sees one name per artist.
fn artists(subtitle: &str) -> String {
    subtitle
        .replace('\u{00A0}', " ")
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The JSON between the `__NEXT_DATA__` script tags, without depending on attribute order.
fn next_data(html: &str) -> Option<&str> {
    let marker = html.find("__NEXT_DATA__")?;
    let open = marker + html[marker..].find('>')? + 1;
    let close = open + html[open..].find("</script>")?;
    Some(&html[open..close])
}

#[derive(Deserialize)]
struct NextData {
    props: Props,
}

#[derive(Deserialize)]
struct Props {
    #[serde(rename = "pageProps")]
    page_props: PageProps,
}

#[derive(Deserialize)]
struct PageProps {
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    state: Option<State>,
}

#[derive(Deserialize)]
struct State {
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    #[serde(default)]
    entity: Option<Entity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entity {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    cover_art: Option<Cover>,
    #[serde(default)]
    track_list: Vec<Row>,
    #[serde(default)]
    is_playable: Option<bool>,
}

#[derive(Deserialize)]
struct Cover {
    #[serde(default)]
    sources: Vec<Image>,
}

#[derive(Deserialize)]
struct Image {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    #[serde(default)]
    uri: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    duration: Option<u64>,
    #[serde(default)]
    is_playable: bool,
    #[serde(default)]
    audio_preview: Option<Preview>,
}

#[derive(Deserialize)]
struct Preview {
    #[serde(default)]
    #[allow(dead_code)]
    url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(rows: usize, name: &str) -> String {
        let tracks: Vec<String> = (0..rows)
            .map(|index| {
                format!(
                    r#"{{"uri":"spotify:track:id{index}","title":"Song {index}","subtitle":"Artist One,\u00a0Artist Two","duration":180000,"isPlayable":true,"entityType":"track"}}"#
                )
            })
            .collect();
        format!(
            r##"<html><head><script id="__NEXT_DATA__" type="application/json">{{"props":{{"pageProps":{{"status":200,"state":{{"data":{{"entity":{{"type":"playlist","name":"{name}","title":"{name}","subtitle":"owner","coverArt":{{"sources":[{{"url":"https://art.example/cover.jpg"}}]}},"isPlayable":true,"trackList":[{}]}}}}}}}}}}}}</script></head></html>"##,
            tracks.join(",")
        )
    }

    #[test]
    fn reads_the_public_page_without_an_account() {
        let playlist = parse("3nBdBqQO6hVxIR6TMwF52H", &page(2, "idgaf world!")).unwrap();
        assert_eq!(playlist.source, "spotify");
        assert_eq!(playlist.id, "3nBdBqQO6hVxIR6TMwF52H");
        assert_eq!(playlist.name, "idgaf world!");
        assert_eq!(playlist.owner, "owner");
        assert_eq!(
            playlist.cover.as_deref(),
            Some("https://art.example/cover.jpg")
        );
        assert_eq!(
            playlist.source_url,
            "https://open.spotify.com/playlist/3nBdBqQO6hVxIR6TMwF52H"
        );
        assert_eq!(playlist.entries.len(), 2);
        assert!(!playlist.limited);
        let first = &playlist.entries[0];
        assert_eq!(first.title, "Song 0");
        assert_eq!(first.artists, "Artist One, Artist Two");
        assert_eq!(first.duration, Duration::from_secs(180));
        assert_eq!(first.id.as_deref(), Some("id0"));
        assert_eq!(
            first.url.as_deref(),
            Some("https://open.spotify.com/track/id0")
        );
        assert!(first.playable);
        assert!(!first.preview);
    }

    #[test]
    fn a_full_page_is_flagged_as_limited() {
        let playlist = parse("large", &page(EMBED_LIMIT, "Large")).unwrap();
        assert_eq!(playlist.entries.len(), EMBED_LIMIT);
        assert!(playlist.limited);
    }

    #[test]
    fn a_missing_playlist_is_not_found() {
        let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"status":404}}}</script>"#;
        assert_eq!(parse("gone", html), Err(ImportFailure::NotFound));
    }

    #[test]
    fn an_unpublished_track_list_is_reported_as_unavailable() {
        let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"type":"playlist","name":"Secret","isPlayable":false,"trackList":[]}}}}}}</script>"#;
        assert_eq!(
            parse("secret", html),
            Err(ImportFailure::TracklistUnavailable)
        );
    }

    #[test]
    fn an_empty_public_playlist_is_empty_not_missing() {
        let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"type":"playlist","name":"Empty","isPlayable":true,"trackList":[]}}}}}}</script>"#;
        assert_eq!(parse("empty", html), Err(ImportFailure::EmptySource));
    }

    #[test]
    fn a_changed_page_is_reported_without_inventing_a_playlist() {
        assert_eq!(
            parse("changed", "<html>no payload</html>"),
            Err(ImportFailure::PublicPageChanged)
        );
    }

    #[test]
    fn preview_only_rows_keep_the_preview_marker() {
        let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"type":"playlist","name":"Clips","isPlayable":true,"trackList":[{"uri":"spotify:track:x","title":"Clip","subtitle":"A","duration":30000,"isPlayable":false,"audioPreview":{"url":"https://p.scdn.co/clip"}}]}}}}}}</script>"#;
        let playlist = parse("clips", html).unwrap();
        assert_eq!(playlist.entries.len(), 1);
        assert!(!playlist.entries[0].playable);
        assert!(playlist.entries[0].preview);
    }
}
