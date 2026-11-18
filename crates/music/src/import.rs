//! Reading a playlist link from another service, and the vocabulary every importer shares.
//!
//! Nothing here reaches the network on its own. The Spotify Web API client lives in
//! [`crate::spotify::web`]; this module holds the link parser, the status model and the
//! title/artist matching rules, so the persistence and the screen can be tested without an
//! account.

use std::fmt;
use std::time::Duration;

use crate::Track;
use crate::models::ImportedEntry;

/// How much two recordings of the same song may differ in length and still be the same take.
/// Spotify rounds to the second, and a local file's tag may round differently.
pub const SAME_RECORDING: Duration = Duration::from_secs(5);

/// What made an import fail, in a form the dialog can explain without reading an English log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportFailure {
    /// The text is not a playlist link or URI.
    InvalidLink,
    /// Spotify is not the provider the app is playing through, so there is no session to ask.
    SpotifyNotConnected,
    /// The token was refused. Signing in again is the fix.
    Unauthorized,
    /// The playlist is private, or the account may not read it. Soundy never bypasses this.
    Forbidden,
    NotFound,
    /// Spotify asked the client to wait. The wait was honoured before giving up.
    RateLimited {
        seconds: u64,
    },
    Cancelled,
    Network(String),
    Other(String),
}

impl fmt::Display for ImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLink => formatter.write_str("that is not a Spotify playlist link"),
            Self::SpotifyNotConnected => {
                formatter.write_str("importing needs Spotify connected and selected as the active source")
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
            Self::Network(reason) => write!(formatter, "cannot reach Spotify: {reason}"),
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

/// A recording on Spotify as the Web API described it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpotifyEntry {
    pub id: Option<String>,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub duration: Duration,
    pub cover: Option<String>,
    pub isrc: Option<String>,
    pub url: Option<String>,
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
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub cover: Option<String>,
    pub source_url: String,
    /// The count the source reports, which may exceed `entries` if one was skipped.
    pub total: usize,
    pub entries: Vec<SpotifyEntry>,
}

impl SpotifyPlaylist {
    /// Turns the fetched playlist into entries carrying their status and matched track.
    /// `local` is the listener's library, `isrc` a lazy reader used only for an ambiguous
    /// local candidate, and `provider_match` an optional fallback for a non-Spotify provider.
    pub fn into_import(
        self,
        local: &[Track],
        isrc: &mut dyn FnMut(&str) -> Option<String>,
        mut provider_match: impl FnMut(&SpotifyEntry) -> Option<Track>,
        imported_at: i64,
    ) -> crate::models::ImportedPlaylistData {
        let mut duplicates = 0_u32;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut entries = Vec::with_capacity(self.entries.len());

        for (position, entry) in self.entries.into_iter().enumerate() {
            let key = entry
                .id
                .clone()
                .or_else(|| entry.isrc.clone())
                .unwrap_or_else(|| format!("{}|{}", key(&entry.title), key(&entry.artists)));
            if !seen.insert(key) {
                duplicates += 1;
            }

            let (status, track_id) = if let Some(found) = match_local(&entry, local, isrc) {
                (crate::models::ImportStatus::Personal, found.id.clone())
            } else if let Some(found) = provider_match(&entry) {
                (crate::models::ImportStatus::Available, found.id.clone())
            } else if entry.playable && entry.id.is_some() {
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

            entries.push(ImportedEntry {
                position: position as u32,
                source_id: entry.id,
                source_url: entry.url,
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

        crate::models::ImportedPlaylistData {
            source: "spotify".to_owned(),
            source_id: self.id,
            name: self.name,
            description: self.description,
            owner: self.owner,
            source_url: self.source_url,
            cover_url: self.cover,
            imported_at,
            entries,
            duplicates,
        }
    }
}

/// The local files whose title and credited artist agree with `entry`, whatever their length.
/// An importer reads their ISRC tags only for this short list rather than the whole library.
pub fn local_candidates<'a>(entry: &SpotifyEntry, local: &'a [Track]) -> Vec<&'a Track> {
    let title = key(&entry.title);
    if title.is_empty() {
        return Vec::new();
    }
    let artists = split_artists(&entry.artists);
    local
        .iter()
        .filter(|track| key(&track.name) == title)
        .filter(|track| same_artist(&artists, &split_artists(&track.artists)))
        .collect()
}

/// The best local file for `entry`: the album's own cut when there is one, then the first
/// exact title/artist/duration match, then an ISRC match among title/artist candidates. A
/// remix, live take or cover differs in its title and is never substituted for the recording.
pub fn match_local<'a>(
    entry: &SpotifyEntry,
    local: &'a [Track],
    isrc: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<&'a Track> {
    let album = key(&entry.album);
    let candidates = local_candidates(entry, local);
    if candidates.is_empty() {
        return None;
    }

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

/// Whether two lengths can be the same recording. A missing length on either side counts as
/// unknown, not as a mismatch.
fn close(left: Duration, right: Duration) -> bool {
    let left = left.as_secs();
    let right = right.as_secs();
    left == 0 || right == 0 || left.abs_diff(right) <= SAME_RECORDING.as_secs()
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
        let entry = SpotifyEntry {
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
        let entry = SpotifyEntry {
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
        let entry = SpotifyEntry {
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
        let playlist = SpotifyPlaylist {
            id: "list".into(),
            name: "List".into(),
            source_url: "https://open.spotify.com/playlist/list".into(),
            entries: vec![
                SpotifyEntry {
                    id: Some("one".into()),
                    title: "One".into(),
                    playable: true,
                    ..Default::default()
                },
                SpotifyEntry {
                    id: Some("one".into()),
                    title: "One".into(),
                    playable: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let import = playlist.into_import(&[], &mut |_| None, |_| None, 0);
        assert_eq!(import.entries.len(), 2);
        assert_eq!(import.duplicates, 1);
        assert!(import.entries.iter().all(|entry| entry.status.playable()));
    }
}
