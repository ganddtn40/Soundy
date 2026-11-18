//! One import of a playlist from a link, a list file or a pasted list, and the playlist it
//! becomes in the library.
//!
//! The dialog owns the text and the file choice; this entity owns the work. Opening the dialog
//! does nothing until Import is pressed, so an idle app makes no request at all. Reading,
//! matching and the database write run away from the UI thread, and only the counts they
//! publish are read while they run. No path needs a Spotify account: a link with no authorized
//! session reports that Spotify does not publish its tracklist and leaves the file and pasted
//! list paths available.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, EventEmitter, Task};
use music::audius::Catalogue;
use music::import::{
    ImportFailure, MAX_IMPORT_BYTES, SourcePlaylist, SpotifyLink, match_local, parse_csv,
    parse_m3u, parse_manual, parse_spotify_link, search_query, track_matches,
};
use music::{ImportStatus, ImportedEntry, ImportedPlaylistData, Track};

use crate::{Io, Library, Session, Shelf, Sonora, join};

/// How often the dialog is told the counts moved while the import runs. Nothing polls an idle
/// import, and the count is only reported when it has changed.
const PROGRESS_TICK: Duration = Duration::from_millis(180);
/// A large playlist may need this many ISRC lookups for ambiguous local candidates. Past it the
/// remaining candidates are matched by title, artist and length alone.
const ISRC_READS: usize = 500;
/// How many provider and catalogue lookups run at once. More only piles requests onto the same
/// two services without making a large list finish sooner.
const LOOKUPS: usize = 4;
/// The most distinct search answers one import keeps. A playlist with more distinct songs than
/// this still matches; it simply asks again rather than growing without a bound.
const LOOKUP_CACHE: usize = 2048;

/// What an import did, in the numbers the result screen shows.
#[derive(Clone, Debug)]
pub struct ImportReport {
    pub playlist_id: String,
    /// `spotify`, `csv`, `m3u` or `manual`.
    pub source: String,
    pub name: String,
    pub cover: Option<String>,
    pub source_url: String,
    /// The source's own description, kept with the playlist and shown once on the result.
    pub description: String,
    pub total: usize,
    pub available: usize,
    pub personal: usize,
    pub preview: usize,
    pub external: usize,
    pub unavailable: usize,
    /// The entries the listener cannot play, in their source order.
    pub unmatched: Vec<ImportedEntry>,
    pub duplicates: u32,
    /// Rows the list carried that could not be read as a track.
    pub skipped: usize,
    /// Rows that now have a picture, whether the export brought it or it was found.
    pub artwork: usize,
    /// Rows that can still show a picture and have none, which is what a retry can change.
    pub missing_art: usize,
    /// Rows whose picture or missing length an enrichment lookup supplied.
    pub enriched: usize,
    /// Whether a collage or a drawn card became the playlist's own cover.
    pub collage: bool,
    /// When the import ran, in seconds since the epoch.
    pub imported_at: i64,
}

impl ImportReport {
    pub fn playable(&self) -> usize {
        self.available + self.personal
    }

    fn of(data: &ImportedPlaylistData, playlist_id: String) -> Self {
        let mut report = Self {
            playlist_id,
            source: data.source.clone(),
            name: data.name.clone(),
            cover: data.cover_url.clone(),
            source_url: data.source_url.clone(),
            description: data.description.clone(),
            total: data.entries.len(),
            available: 0,
            personal: 0,
            preview: 0,
            external: 0,
            unavailable: 0,
            unmatched: Vec::new(),
            duplicates: data.duplicates,
            skipped: data.skipped,
            artwork: data
                .entries
                .iter()
                .filter(|entry| entry.cover.is_some())
                .count(),
            missing_art: data
                .entries
                .iter()
                .filter(|entry| entry.cover.is_none() && displayable(entry.status))
                .count(),
            enriched: 0,
            collage: false,
            imported_at: data.imported_at,
        };
        for entry in &data.entries {
            match entry.status {
                ImportStatus::Available => report.available += 1,
                ImportStatus::Personal => report.personal += 1,
                ImportStatus::Preview => report.preview += 1,
                ImportStatus::External => report.external += 1,
                ImportStatus::Unavailable => report.unavailable += 1,
            }
            if !entry.status.playable() {
                report.unmatched.push(entry.clone());
            }
        }
        report
    }
}

/// Whether a row can show artwork at all. An external link or an unavailable row never will,
/// so a retry must not offer to find one for it.
fn displayable(status: ImportStatus) -> bool {
    matches!(
        status,
        ImportStatus::Available | ImportStatus::Personal | ImportStatus::Preview
    )
}

/// Where an import has got to. The dialog reads this rather than being told, so a redraw for
/// any other reason never loses its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Fetching,
    Matching,
    /// Looking up the pictures and missing lengths the list did not carry.
    Enriching,
    Saving,
    Done,
    Failed,
}

pub enum ImportEvent {
    Changed,
}

pub struct Import {
    stage: Stage,
    /// The counts the fetch and match have published, written by the background task.
    read: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    /// What the dialog was last told, so a tick with no move notifies nobody.
    shown: (usize, usize),
    cancel: Arc<AtomicBool>,
    report: Option<Arc<ImportReport>>,
    failure: Option<ImportFailure>,
    /// The text that failed, so the field can keep it after a dialog is reopened.
    link: String,
    task: Option<Task<()>>,
    ticker: Option<Task<()>>,
    session: gpui::Entity<Session>,
    library: gpui::Entity<Library>,
}

impl EventEmitter<ImportEvent> for Import {}

impl Import {
    pub fn new(
        session: gpui::Entity<Session>,
        library: gpui::Entity<Library>,
        cx: &mut Context<Self>,
    ) -> Self {
        // A signed-out account leaves a result that speaks for an account no longer here.
        cx.subscribe(&session, |this, _, event, cx| {
            if let crate::SessionEvent::SignedOut = event
                && matches!(this.stage, Stage::Done | Stage::Failed)
            {
                this.clear();
                cx.notify();
            }
        })
        .detach();

        Self {
            stage: Stage::Idle,
            read: Arc::new(AtomicUsize::new(0)),
            total: Arc::new(AtomicUsize::new(0)),
            shown: (0, 0),
            cancel: Arc::new(AtomicBool::new(false)),
            report: None,
            failure: None,
            link: String::new(),
            task: None,
            ticker: None,
            session,
            library,
        }
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn active(&self) -> bool {
        matches!(
            self.stage,
            Stage::Fetching | Stage::Matching | Stage::Enriching | Stage::Saving
        )
    }

    pub fn read(&self) -> usize {
        self.read.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> usize {
        self.total.load(Ordering::Relaxed)
    }

    pub fn report(&self) -> Option<&ImportReport> {
        self.report.as_deref()
    }

    pub fn failure(&self) -> Option<&ImportFailure> {
        self.failure.as_ref()
    }

    /// The link or path the last attempt used, so reopening the dialog can put it back.
    pub fn link(&self) -> &str {
        &self.link
    }

    fn clear(&mut self) {
        self.stage = Stage::Idle;
        self.report = None;
        self.failure = None;
        self.link.clear();
        self.read.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.shown = (0, 0);
        self.task = None;
        self.ticker = None;
    }

    /// Returns the dialog to its field after a result, leaving the imported playlist alone.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        if self.active() {
            return;
        }
        self.clear();
        cx.notify();
    }

    /// Asks whatever the import is doing to stop. The fetch checks this between requests and
    /// during a rate-limit wait; the write is one transaction and is never cut in half.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.active() {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        cx.notify();
    }

    /// Whether the last result still has rows without a picture, or rows whose length the
    /// source never gave, so another look could change something.
    pub fn can_retry_artwork(&self) -> bool {
        self.stage == Stage::Done
            && self
                .report
                .as_deref()
                .is_some_and(|report| report.missing_art > 0)
    }

    /// Looks again for the pictures and missing lengths of the playlist the last import saved.
    /// Only the saved rows are read, so nothing is matched or written a second time; the
    /// playlist's own generated cover is rebuilt when the pass finds more art, and a picture
    /// the listener chose is left alone. Answers nothing while another import is running.
    pub fn retry_artwork(&mut self, cx: &mut Context<Self>) {
        if self.active() {
            return;
        }
        let Some(report) = self.report.clone() else {
            return;
        };
        let Some(local) = self.session.read(cx).local_client() else {
            return;
        };
        let id = report.playlist_id.clone();
        let cancel = self.cancel.clone();
        cancel.store(false, Ordering::Relaxed);
        self.stage = Stage::Enriching;
        self.read.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.shown = (0, 0);
        cx.notify();

        let io = Io::global(cx);
        let library = self.library.clone();
        let catalog = self.session.read(cx).catalog(music::LOCAL_PLAYLIST_PREFIX);

        self.task = Some(cx.spawn(async move |this, cx| {
            let refreshed = join(io.spawn({
                let local = local.clone();
                let id = id.clone();
                let cancel = cancel.clone();
                async move { local.refresh_imported_art(&id, &cancel).await }
            }))
            .await;

            let refresh = match refreshed {
                Ok(refresh) => refresh,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        // The playlist as saved is still whole, so the result is kept and the
                        // failure is only logged. A retry that cannot reach a catalogue has
                        // found nothing; it has not lost anything.
                        log::warn!("import: the artwork retry could not run: {error:#}");
                        this.stage = Stage::Done;
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };

            let cover = join(io.spawn({
                let local = local.clone();
                let id = id.clone();
                async move {
                    local
                        .playlist(&id)
                        .await
                        .map(|detail| detail.playlist.cover)
                }
            }))
            .await
            .ok()
            .flatten();

            this.update(cx, |this, cx| {
                this.stage = Stage::Done;
                if let Some(report) = this.report.as_deref().cloned() {
                    let mut report = report;
                    if let Some(refresh) = &refresh {
                        report.artwork = report.artwork.saturating_add(refresh.covers);
                        report.missing_art = refresh.missing;
                        report.enriched += refresh.covers + refresh.durations;
                        report.collage |= refresh.collage;
                    }
                    if cover.is_some() {
                        report.cover = cover;
                    }
                    this.report = Some(Arc::new(report));
                }
                if let Some(catalog) = catalog {
                    let changed = id.clone();
                    // The task runs on the pool; dropping its handle only means nobody waits
                    // for the page cache to be cleared before the dialog shows the result.
                    drop(io.spawn(async move { catalog.invalidate_playlist(&changed).await }));
                }
                library.update(cx, |library, cx| {
                    library.imported_playlist_replaced(&id, cx);
                });
                cx.notify();
                cx.emit(ImportEvent::Changed);
            })
            .ok();
        }));
    }

    /// Starts importing a Spotify playlist link or URI. This is the only path that can use a
    /// Spotify session, and only when one is already authorized. Without it the dialog is told
    /// why the tracklist is unavailable and the file/pasted-list paths remain usable.
    pub fn start_link(&mut self, input: &str, cx: &mut Context<Self>) {
        if self.active() {
            return;
        }
        let link = match parse_spotify_link(input) {
            Ok(link) => link,
            Err(failure) => return self.fail(failure, cx),
        };
        let Some(web) = self
            .session
            .read(cx)
            .client()
            .and_then(|client| client.spotify_web())
        else {
            return self.fail(ImportFailure::TracklistUnavailable, cx);
        };
        let label = input.trim().to_owned();
        let id = link.id.clone();
        self.start_with(
            label,
            move |cancel, read, total| async move {
                let progress = move |seen: usize, all: usize| {
                    read.store(seen, Ordering::Relaxed);
                    total.store(all, Ordering::Relaxed);
                };
                web.playlist(&id, &cancel, &progress).await
            },
            cx,
        );
    }

    /// Reads an M3U, M3U8, CSV or text list. The extension selects the parser; nothing is
    /// guessed from the file's contents beyond that. The read and parse run on the blocking
    /// pool.
    pub fn start_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.active() {
            return;
        }
        let label = path.display().to_string();
        let io = Io::global(cx);
        self.start_with(
            label,
            move |_, _, _| async move {
                match io.spawn_blocking(move || load_file(&path)).await {
                    Ok(parsed) => parsed,
                    Err(error) => Err(ImportFailure::File(format!(
                        "cannot read the playlist file: {error}"
                    ))),
                }
            },
            cx,
        );
    }

    /// Reads tracks pasted from the clipboard, one per line. The content is hashed so importing
    /// the same list twice updates the same local playlist instead of making a second copy.
    pub fn start_manual(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.active() {
            return;
        }
        let text = text.trim().to_owned();
        if text.is_empty() {
            return self.fail(ImportFailure::EmptySource, cx);
        }
        let io = Io::global(cx);
        self.start_with(
            "pasted track list".to_owned(),
            move |_, _, _| async move {
                match io
                    .spawn_blocking(move || {
                        let mut hasher = DefaultHasher::new();
                        text.hash(&mut hasher);
                        let mut playlist = parse_manual(&text, "Pasted track list")?;
                        playlist.id = format!("manual:{:016x}", hasher.finish());
                        playlist.description = "Imported from a pasted track list".to_owned();
                        Ok(playlist)
                    })
                    .await
                {
                    Ok(parsed) => parsed,
                    Err(error) => Err(ImportFailure::File(format!(
                        "cannot read the pasted list: {error}"
                    ))),
                }
            },
            cx,
        );
    }

    fn start_with<F, M>(&mut self, label: String, make: M, cx: &mut Context<Self>)
    where
        F: Future<Output = Result<SourcePlaylist, ImportFailure>> + Send + 'static,
        M: FnOnce(Arc<AtomicBool>, Arc<AtomicUsize>, Arc<AtomicUsize>) -> F,
    {
        if self.active() {
            return;
        }
        self.clear();

        let Some(local) = self.session.read(cx).local_client() else {
            return self.fail(
                ImportFailure::Other(
                    "the local library is still starting; try again in a moment".to_owned(),
                ),
                cx,
            );
        };
        let provider = self.session.read(cx).client();

        let cancel = self.cancel.clone();
        let read = self.read.clone();
        let total = self.total.clone();
        cancel.store(false, Ordering::Relaxed);
        let fetch = make(cancel.clone(), read.clone(), total.clone());

        self.link = label;
        self.stage = Stage::Fetching;
        self.read.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.shown = (0, 0);
        cx.notify();

        let io = Io::global(cx);
        let library = self.library.clone();
        // Imported playlists are always local ones, and their page may already be cached from
        // before the import wrote them.
        let catalog = self.session.read(cx).catalog(music::LOCAL_PLAYLIST_PREFIX);

        self.task = Some(cx.spawn(async move |this, cx| {
            let playlist = match io.spawn(fetch).await {
                Ok(Ok(playlist)) => playlist,
                Ok(Err(failure)) => {
                    this.update(cx, |this, cx| this.fail(failure, cx)).ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.fail(ImportFailure::Network(error.to_string()), cx)
                    })
                    .ok();
                    return;
                }
            };

            this.update(cx, |this, cx| {
                this.stage = Stage::Matching;
                this.total.store(playlist.entries.len(), Ordering::Relaxed);
                this.read.store(0, Ordering::Relaxed);
                cx.notify();
            })
            .ok();

            let local_tracks: Vec<Track> = match join(io.spawn({
                let local = local.clone();
                async move { local.all_tracks().await }
            }))
            .await
            {
                Ok(tracks) => tracks,
                Err(error) => {
                    log::warn!("import: cannot read the local library: {error:#}");
                    Vec::new()
                }
            };

            // Only recordings whose title and artist already agree need an ISRC read, and only
            // when the lengths disagree. A library of thousands of files otherwise costs
            // thousands of tag reads for an answer title and length already give.
            let candidates: Vec<String> = {
                let mut seen = HashSet::new();
                let mut ids = Vec::new();
                for entry in &playlist.entries {
                    if entry.isrc.is_none() {
                        continue;
                    }
                    for track in music::import::local_candidates(entry, &local_tracks) {
                        if ids.len() >= ISRC_READS {
                            break;
                        }
                        if let Some(id) = track.id.clone()
                            && seen.insert(id.clone())
                        {
                            ids.push(id);
                        }
                    }
                }
                ids
            };

            let isrc = join(io.spawn({
                let local = local.clone();
                async move {
                    let mut found: HashMap<String, String> = HashMap::new();
                    for id in candidates {
                        if let Ok(tags) = local.track_tags(&id).await
                            && !tags.isrc.is_empty()
                        {
                            found.insert(id, tags.isrc);
                        }
                    }
                    anyhow::Ok(found)
                }
            }))
            .await
            .unwrap_or_default();

            // The local match is the expensive half of a large import, so it runs on the
            // blocking pool rather than between network awaits on the window's thread. The
            // answer also tells the network stage which rows need no lookup at all.
            let computed = io
                .spawn_blocking({
                    let isrc = isrc;
                    let local_tracks = local_tracks;
                    let playlist = playlist;
                    move || {
                        let mut read = |id: &str| isrc.get(id).cloned();
                        let local_matches: Vec<Option<String>> = playlist
                            .entries
                            .iter()
                            .map(|entry| {
                                match_local(entry, &local_tracks, &mut read)
                                    .and_then(|track| track.id.clone())
                            })
                            .collect();
                        (playlist, local_matches)
                    }
                })
                .await;
            let (mut playlist, local_matches) = match computed {
                Ok(value) => value,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.fail(
                            ImportFailure::Other(format!("cannot match the playlist: {error}")),
                            cx,
                        )
                    })
                    .ok();
                    return;
                }
            };

            if cancel.load(Ordering::Relaxed) {
                this.update(cx, |this, cx| this.fail(ImportFailure::Cancelled, cx))
                    .ok();
                return;
            }

            let mut provider_matches: Vec<Option<Track>> = vec![None; playlist.entries.len()];
            let mut external_matches: Vec<Option<String>> = vec![None; playlist.entries.len()];
            let mut provider_cache: HashMap<String, Vec<Track>> = HashMap::new();
            let mut external_cache: HashMap<String, Option<String>> = HashMap::new();
            let mut provider_up = provider.is_some();
            let mut catalogues_up = playlist.source != "spotify";
            let catalogues = Arc::new(Catalogue::public());
            let searches = playlist.source != "spotify";

            // A row that needs a search asks its provider and then a public catalogue. Rows are
            // matched in chunks of LOOKUPS so a large playlist finishes in a fraction of the
            // round trips without opening hundreds of sockets at once. Answers are returned to
            // their own row, so a playlist keeps its order whatever order the network replies.
            for start in (0..local_matches.len()).step_by(LOOKUPS) {
                if cancel.load(Ordering::Relaxed) {
                    this.update(cx, |this, cx| this.fail(ImportFailure::Cancelled, cx))
                        .ok();
                    return;
                }
                let end = (start + LOOKUPS).min(local_matches.len());
                // A row with no credited artist and no ISRC cannot be told apart from an
                // unrelated recording, so no provider or catalogue search can accept it; it
                // stays an external or unavailable row without spending a request.
                let positions = || {
                    (start..end).filter(|&position| {
                        searches
                            && local_matches[position].is_none()
                            && (!playlist.entries[position].artists.trim().is_empty()
                                || playlist.entries[position].isrc.is_some())
                    })
                };

                if provider_up && let Some(client) = provider.clone() {
                    let mut pending: Vec<(usize, String)> = Vec::new();
                    for position in positions() {
                        let entry = &playlist.entries[position];
                        let query = search_query(entry);
                        match provider_cache.get(&query) {
                            Some(results) => {
                                provider_matches[position] = provider_match(entry, results);
                            }
                            None => pending.push((position, query)),
                        }
                    }
                    let mut queries: Vec<String> = Vec::new();
                    for (_, query) in &pending {
                        if !queries.contains(query) {
                            queries.push(query.clone());
                        }
                    }
                    // The providers talk through tokio, whose reactor the window's executor does
                    // not have; the searches run on the io runtime that owns the connections.
                    let answers = match io
                        .spawn(async move {
                            futures::future::join_all(queries.into_iter().map(|query| {
                                let client = client.clone();
                                async move {
                                    let result = client.search(&query).await;
                                    (query, result)
                                }
                            }))
                            .await
                        })
                        .await
                    {
                        Ok(answers) => answers,
                        Err(error) => {
                            log::debug!("import: the provider search could not run: {error}");
                            provider_up = false;
                            Vec::new()
                        }
                    };
                    let mut found: HashMap<String, Vec<Track>> = HashMap::new();
                    for (query, result) in answers {
                        match result {
                            Ok(results) => {
                                if provider_cache.len() < LOOKUP_CACHE
                                    || provider_cache.contains_key(&query)
                                {
                                    provider_cache.insert(query.clone(), results.clone());
                                }
                                found.insert(query, results);
                            }
                            Err(error) => {
                                log::debug!("import: the active provider cannot search: {error:#}");
                                provider_up = false;
                            }
                        }
                    }
                    for (position, query) in pending {
                        provider_matches[position] = found.get(&query).and_then(|results| {
                            provider_match(&playlist.entries[position], results)
                        });
                    }
                }

                if catalogues_up {
                    let mut pending: Vec<(usize, String, music::import::SourceEntry)> = Vec::new();
                    for position in positions() {
                        if provider_matches[position].is_some() {
                            continue;
                        }
                        let entry = &playlist.entries[position];
                        let query = search_query(entry);
                        match external_cache.get(&query) {
                            Some(found) => external_matches[position] = found.clone(),
                            None => pending.push((position, query, entry.clone())),
                        }
                    }
                    let mut queries: Vec<(String, music::import::SourceEntry)> = Vec::new();
                    for (_, query, entry) in &pending {
                        if !queries.iter().any(|(known, _)| known == query) {
                            queries.push((query.clone(), entry.clone()));
                        }
                    }
                    let answers = match io
                        .spawn({
                            let catalogues = catalogues.clone();
                            async move {
                                futures::future::join_all(queries.into_iter().map(
                                    |(query, entry)| {
                                        let catalogues = catalogues.clone();
                                        async move {
                                            let found = catalogues.find(&entry).await;
                                            (query, found)
                                        }
                                    },
                                ))
                                .await
                            }
                        })
                        .await
                    {
                        Ok(answers) => answers,
                        Err(error) => {
                            log::debug!("import: the catalogue lookups could not run: {error}");
                            catalogues_up = false;
                            Vec::new()
                        }
                    };
                    let mut found: HashMap<String, Option<String>> = HashMap::new();
                    for (query, result) in answers {
                        match result {
                            Ok(url) => {
                                if external_cache.len() < LOOKUP_CACHE
                                    || external_cache.contains_key(&query)
                                {
                                    external_cache.insert(query.clone(), url.clone());
                                }
                                found.insert(query, url);
                            }
                            Err(error) => {
                                log::debug!("import: public catalogue unavailable: {error:#}");
                                catalogues_up = false;
                            }
                        }
                    }
                    for (position, query, _) in pending {
                        if let Some(found) = found.get(&query) {
                            external_matches[position] = found.clone();
                        }
                    }
                }

                read.store(end, Ordering::Relaxed);
            }

            if cancel.load(Ordering::Relaxed) {
                this.update(cx, |this, cx| this.fail(ImportFailure::Cancelled, cx))
                    .ok();
                return;
            }

            // An export usually carries names and lengths but no picture; a pasted list may
            // carry neither the picture nor the length. Rows are grouped by album, so this is
            // a few dozen lookups for a large list rather than one per song. A Spotify link
            // already brought its own artwork and is left alone.
            let mut enrichment = music::enrich::Report::default();
            if matches!(playlist.source.as_str(), "csv" | "manual") {
                this.update(cx, |this, cx| {
                    this.stage = Stage::Enriching;
                    this.read.store(0, Ordering::Relaxed);
                    this.total.store(0, Ordering::Relaxed);
                    this.shown = (0, 0);
                    cx.notify();
                })
                .ok();

                let enricher = music::enrich::Enricher::public();
                let entries = playlist.entries.clone();
                let cancelled = cancel.clone();
                let done = read.clone();
                let all = total.clone();
                let computed = io
                    .spawn(async move {
                        let mut entries = entries;
                        let progress = |done_at: usize, total_at: usize| {
                            done.store(done_at, Ordering::Relaxed);
                            all.store(total_at, Ordering::Relaxed);
                        };
                        let report = enricher.enrich(&mut entries, &cancelled, &progress).await;
                        (entries, report)
                    })
                    .await;
                match computed {
                    Ok((entries, report)) => {
                        playlist.entries = entries;
                        enrichment = report;
                    }
                    Err(error) => {
                        log::warn!("import: the artwork lookups could not run: {error}");
                    }
                }
            }

            if cancel.load(Ordering::Relaxed) {
                this.update(cx, |this, cx| this.fail(ImportFailure::Cancelled, cx))
                    .ok();
                return;
            }

            this.update(cx, |this, cx| {
                this.stage = Stage::Saving;
                cx.notify();
            })
            .ok();

            // Every match is known now; assembling the rows only copies strings and runs on the
            // window's thread without a wait.
            let data =
                playlist.into_import(&local_matches, &provider_matches, &external_matches, now());

            let saved = join(io.spawn({
                let local = local.clone();
                let data = data.clone();
                async move { local.save_import(&data).await }
            }))
            .await;

            let playlist_id = match saved {
                Ok(id) => id,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.fail(
                            ImportFailure::Other(format!("cannot keep the playlist: {error:#}")),
                            cx,
                        )
                    })
                    .ok();
                    return;
                }
            };

            // A playlist imported from a list has no picture of its own yet. The local client
            // builds one from the first four albums, or draws the emerald card when the list
            // brought none, and keeps it as the playlist's own cover so the header, list and
            // player agree. A link import already carries the service's cover and keeps it.
            let mut collage = false;
            let mut own_cover = None;
            if data.cover_url.is_none() {
                match join(io.spawn({
                    let local = local.clone();
                    let id = playlist_id.clone();
                    async move { local.build_import_cover(&id).await }
                }))
                .await
                {
                    Ok(built) => collage = built,
                    Err(error) => log::warn!("import: cannot build the playlist cover: {error:#}"),
                }
                if collage {
                    own_cover = join(io.spawn({
                        let local = local.clone();
                        let id = playlist_id.clone();
                        async move {
                            local
                                .playlist(&id)
                                .await
                                .map(|detail| detail.playlist.cover)
                        }
                    }))
                    .await
                    .ok()
                    .flatten();
                }
            }

            // The page read before the save is now out of date, and a second import of the
            // same source replaces the playlist under the same id. Drop the cached page so
            // the page opens on the rows this import wrote, and let an open page read them.
            let changed = playlist_id.clone();
            if let Some(catalog) = catalog {
                let id = changed.clone();
                io.spawn(async move { catalog.invalidate_playlist(&id).await })
                    .await
                    .ok();
            }

            let mut report = ImportReport::of(&data, playlist_id);
            report.enriched = enrichment.enriched();
            report.collage = collage;
            if own_cover.is_some() {
                report.cover = own_cover;
            }
            let report = Arc::new(report);
            this.update(cx, |this, cx| {
                this.stage = Stage::Done;
                this.report = Some(report);
                this.failure = None;
                library.update(cx, |library, cx| {
                    library.refresh(Shelf::Local, cx);
                    library.imported_playlist_replaced(&changed, cx);
                });
                cx.notify();
                cx.emit(ImportEvent::Changed);
            })
            .ok();
        }));

        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_TICK).await;
                let keep = this
                    .update(cx, |this, cx| {
                        if !this.active() {
                            return false;
                        }
                        if this.stage == Stage::Saving {
                            return true;
                        }
                        let shown = (this.read(), this.total());
                        if shown != this.shown {
                            this.shown = shown;
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }

    fn fail(&mut self, failure: ImportFailure, cx: &mut Context<Self>) {
        self.stage = Stage::Failed;
        self.failure = Some(failure);
        self.report = None;
        self.task = None;
        self.ticker = None;
        cx.notify();
        cx.emit(ImportEvent::Changed);
    }

    /// The link parser's outcome, without touching the network. Used by the dialog to keep the
    /// Import button honest before it is pressed.
    pub fn accepts(input: &str) -> Result<SpotifyLink, ImportFailure> {
        parse_spotify_link(input)
    }

    /// The library the import writes to, for a caller that wants to open the result.
    pub fn library(&self) -> gpui::Entity<Library> {
        self.library.clone()
    }
}

/// The first answer from the active provider that is really the same recording. A preview-only
/// or id-less result would give a row that cannot be played, so neither counts.
fn provider_match(entry: &music::import::SourceEntry, results: &[Track]) -> Option<Track> {
    results
        .iter()
        .find(|track| track.playable && track.id.is_some() && track_matches(entry, track))
        .cloned()
}

/// Reads and parses one list file. The extension picks the parser; a `.txt` is a pasted list.
fn load_file(path: &Path) -> Result<SourcePlaylist, ImportFailure> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "playlist".to_owned());
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "m3u" | "m3u8" | "csv" | "txt" | "text") {
        return Err(ImportFailure::UnsupportedFile(name));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| ImportFailure::File(format!("cannot read {name}: {error}")))?;
    if metadata.len() > MAX_IMPORT_BYTES as u64 {
        return Err(ImportFailure::FileTooLarge);
    }
    let bytes = std::fs::read(path)
        .map_err(|error| ImportFailure::File(format!("cannot read {name}: {error}")))?;
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Imported playlist".to_owned());
    let folder = path.parent().unwrap_or_else(|| Path::new("."));

    let mut playlist = match extension.as_str() {
        "m3u" | "m3u8" => parse_m3u(&bytes, folder, &stem)?,
        "csv" => parse_csv(&bytes, &stem)?,
        _ => {
            let text = String::from_utf8_lossy(&bytes);
            parse_manual(&text, &stem)?
        }
    };
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    playlist.id = format!("file:{}", canonical.display());
    playlist.source_url = format!("file://{}", canonical.display());
    playlist.name = stem;
    playlist.description = format!("Imported from {name}");
    Ok(playlist)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}

impl Sonora {
    pub fn import(&self) -> &gpui::Entity<Import> {
        &self.imports
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(position: u32, status: ImportStatus) -> ImportedEntry {
        ImportedEntry {
            position,
            title: format!("Song {position}"),
            artists: "Artist".to_owned(),
            album: "Album".to_owned(),
            duration: Duration::from_secs(180),
            status,
            ..Default::default()
        }
    }

    #[test]
    fn a_report_counts_each_status_and_lists_only_unplayable_rows() {
        let mut first = entry(0, ImportStatus::Available);
        first.cover = Some("https://art.example/one.jpg".to_owned());
        let data = ImportedPlaylistData {
            source: "m3u".to_owned(),
            source_id: "list".to_owned(),
            name: "List".to_owned(),
            source_url: "file:///list.m3u".to_owned(),
            imported_at: 1_000,
            entries: vec![
                first,
                entry(1, ImportStatus::Personal),
                entry(2, ImportStatus::Preview),
                entry(3, ImportStatus::External),
                entry(4, ImportStatus::Unavailable),
            ],
            duplicates: 2,
            skipped: 1,
            ..Default::default()
        };
        let report = ImportReport::of(&data, "local:list".to_owned());
        assert_eq!(report.playlist_id, "local:list");
        assert_eq!(report.total, 5);
        assert_eq!(report.playable(), 2);
        assert_eq!(report.available, 1);
        assert_eq!(report.personal, 1);
        assert_eq!(report.preview, 1);
        assert_eq!(report.external, 1);
        assert_eq!(report.unavailable, 1);
        assert_eq!(report.duplicates, 2);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.artwork, 1);
        assert_eq!(report.missing_art, 2);
        assert_eq!(report.enriched, 0);
        assert!(!report.collage);
        assert_eq!(
            report
                .unmatched
                .iter()
                .map(|entry| entry.position)
                .collect::<Vec<_>>(),
            [2, 3, 4]
        );
    }

    #[test]
    fn reads_an_m3u_file_off_disk_and_names_it_after_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Road Trip.m3u8");
        std::fs::write(
            &path,
            "#EXTM3U\n#EXTINF:225,Artist - Song One\none.mp3\n#EXTINF:180,Artist - Song Two\nC:\\Music\\two.flac\n",
        )
        .unwrap();

        let playlist = load_file(&path).unwrap();
        assert_eq!(playlist.source, "m3u");
        assert_eq!(playlist.name, "Road Trip");
        assert_eq!(playlist.description, "Imported from Road Trip.m3u8");
        assert!(playlist.id.starts_with("file:"));
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.entries[0].title, "Song One");
        assert_eq!(playlist.entries[0].duration, Duration::from_secs(225));
        assert_eq!(playlist.entries[1].title, "Song Two");
        assert_eq!(
            playlist.entries[0].local_path.as_deref(),
            Some(dir.path().join("one.mp3").as_path())
        );
    }

    #[test]
    fn reads_a_csv_file_off_disk_in_any_column_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Export.csv");
        std::fs::write(
            &path,
            "Artist,Title,Album,Duration\nArtist One,Song One,Album One,3:45\n",
        )
        .unwrap();

        let playlist = load_file(&path).unwrap();
        assert_eq!(playlist.source, "csv");
        assert_eq!(playlist.name, "Export");
        assert_eq!(playlist.entries.len(), 1);
        assert_eq!(playlist.entries[0].title, "Song One");
        assert_eq!(playlist.entries[0].artists, "Artist One");
        assert_eq!(playlist.entries[0].duration, Duration::from_secs(225));
    }

    #[test]
    fn reads_a_text_file_as_a_pasted_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Notes.txt");
        std::fs::write(&path, "Artist One - Song One\nSong Two | Artist Two\n").unwrap();

        let playlist = load_file(&path).unwrap();
        assert_eq!(playlist.source, "manual");
        assert_eq!(playlist.name, "Notes");
        assert_eq!(playlist.entries.len(), 2);
        assert_eq!(playlist.entries[0].title, "Song One");
        assert_eq!(playlist.entries[1].title, "Song Two");
    }

    #[test]
    fn refuses_a_file_it_has_no_parser_for() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Audio.flac");
        std::fs::write(&path, b"not really audio").unwrap();

        assert_eq!(
            load_file(&path),
            Err(ImportFailure::UnsupportedFile("Audio.flac".to_owned()))
        );
    }

    #[test]
    fn refuses_a_list_with_no_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Empty.csv");
        std::fs::write(&path, "Title,Artist\n").unwrap();

        assert_eq!(load_file(&path), Err(ImportFailure::EmptySource));
    }
}
