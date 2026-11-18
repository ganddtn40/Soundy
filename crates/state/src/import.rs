//! One import of a playlist from another service, and the playlist it becomes in the
//! library.
//!
//! The screen owns the text field; this entity owns the work. Opening the dialog does nothing
//! until Import is pressed, so an idle app makes no request at all. The fetch, the match against
//! the listener's files and the write to the database run away from the UI thread, and only the
//! counts they publish are read while they run.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, EventEmitter, Task};
use music::import::{ImportFailure, SpotifyLink, parse_spotify_link};
use music::{ImportStatus, ImportedEntry, ImportedPlaylistData};

use crate::{Io, Library, Session, Shelf, Sonora, join};

/// How often the dialog is told the counts moved while the fetch runs. Nothing polls an idle
/// import, and the count is only reported when it has changed.
const PROGRESS_TICK: Duration = Duration::from_millis(180);
/// A large playlist may need this many ISRC lookups for ambiguous local candidates. Past it the
/// remaining candidates are matched by title, artist and length alone.
const ISRC_READS: usize = 500;

/// What an import did, in the numbers the result screen shows.
#[derive(Clone, Debug)]
pub struct ImportReport {
    pub playlist_id: String,
    pub name: String,
    pub cover: Option<String>,
    pub source_url: String,
    pub total: usize,
    pub available: usize,
    pub personal: usize,
    pub preview: usize,
    pub external: usize,
    pub unavailable: usize,
    /// The entries the listener cannot play, in their source order.
    pub unmatched: Vec<ImportedEntry>,
    pub duplicates: u32,
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
            name: data.name.clone(),
            cover: data.cover_url.clone(),
            source_url: data.source_url.clone(),
            total: data.entries.len(),
            available: 0,
            personal: 0,
            preview: 0,
            external: 0,
            unavailable: 0,
            unmatched: Vec::new(),
            duplicates: data.duplicates,
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

/// Where an import has got to. The dialog reads this rather than being told, so a redraw for
/// any other reason never loses its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Fetching,
    Matching,
    Saving,
    Done,
    Failed,
}

pub enum ImportEvent {
    Changed,
}

pub struct Import {
    stage: Stage,
    /// The counts the fetch has published, written by the network task.
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
            Stage::Fetching | Stage::Matching | Stage::Saving
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

    /// The link the last attempt used, so reopening the dialog can put it back.
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

    /// Starts importing `input`, a Spotify playlist link or URI. A second press while one runs
    /// is ignored, so a repeated request never fetches the same playlist twice.
    pub fn start(&mut self, input: &str, cx: &mut Context<Self>) {
        if self.active() {
            return;
        }
        self.clear();

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
            return self.fail(ImportFailure::SpotifyNotConnected, cx);
        };
        let Some(local) = self.session.read(cx).local_client() else {
            return self.fail(
                ImportFailure::Other(
                    "the local library is still starting; try again in a moment".to_owned(),
                ),
                cx,
            );
        };

        self.link = input.trim().to_owned();
        self.stage = Stage::Fetching;
        self.cancel.store(false, Ordering::Relaxed);
        self.read.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.shown = (0, 0);
        cx.notify();

        let io = Io::global(cx);
        let library = self.library.clone();
        let cancel = self.cancel.clone();
        let read = self.read.clone();
        let total = self.total.clone();
        let playlist_id = link.id.clone();

        self.task = Some(cx.spawn(async move |this, cx| {
            let fetched = io
                .spawn({
                    let web = web.clone();
                    let id = playlist_id.clone();
                    let cancel = cancel.clone();
                    let read = read.clone();
                    let total = total.clone();
                    async move {
                        web.playlist(&id, &cancel, &move |seen, all| {
                            read.store(seen, Ordering::Relaxed);
                            total.store(all, Ordering::Relaxed);
                        })
                        .await
                    }
                })
                .await;

            let playlist = match fetched {
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
                this.read.store(playlist.entries.len(), Ordering::Relaxed);
                cx.notify();
            })
            .ok();

            let local_tracks = join(io.spawn({
                let local = local.clone();
                async move { local.all_tracks().await }
            }))
            .await
            .unwrap_or_default();

            // Only the recordings whose title and artist already agree need their ISRC read,
            // and only when the lengths disagree. A library of thousands of files otherwise
            // costs thousands of tag reads for an answer title and length already give.
            let candidates: Vec<String> = {
                let mut seen = std::collections::HashSet::new();
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

            if cancel.load(Ordering::Relaxed) {
                this.update(cx, |this, cx| this.fail(ImportFailure::Cancelled, cx))
                    .ok();
                return;
            }

            // Matching a large playlist against a large library is real work; it runs on the
            // IO pool with the library it borrowed, so the window never waits for it.
            let matched = io
                .spawn(async move {
                    let imported_at = now();
                    playlist.into_import(
                        &local_tracks,
                        &mut |id| isrc.get(id).cloned(),
                        |_| None,
                        imported_at,
                    )
                })
                .await;
            let data = match matched {
                Ok(data) => data,
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

            this.update(cx, |this, cx| {
                this.stage = Stage::Saving;
                cx.notify();
            })
            .ok();

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

            let report = Arc::new(ImportReport::of(&data, playlist_id));
            this.update(cx, |this, cx| {
                this.stage = Stage::Done;
                this.report = Some(report);
                this.failure = None;
                library.update(cx, |library, cx| library.refresh(Shelf::Local, cx));
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
                        if this.stage != Stage::Fetching {
                            return this.active();
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
