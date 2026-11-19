//! The dialog that imports a playlist from a Spotify link, an M3U/CSV list or a pasted list.
//!
//! The dialog owns the fields and the screen; the work lives in `state::Import`. Opening it
//! makes no request: reading starts when Import is pressed, and everything the network and the
//! database do runs away from the UI thread. A Spotify link is the only path that can use a
//! Spotify session, and it is never required: a file or a pasted list imports with no account.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FocusHandle, Global, PathPromptOptions, Render, SharedString, Task,
    Window, div, px, svg,
};
use i18n::t;
use music::import::{ImportFailure, MAX_IMPORT_BYTES, parse_manual, parse_spotify_link};
use router::{Destination, navigate};
use state::{ImportReport, ImportStage, Review, ReviewRow, Soundy};
use ui::{
    ActiveTheme as _, Artwork, Button, Dismiss, Field, Form, Input, Modal, Notice, Spinner, Submit,
    TabBar, Text, clock,
};

/// How many unmatched rows the result lists before it says the rest are not shown. A playlist
/// of thousands is still readable; the modal scrolls through the list.
const UNMATCHED_SHOWN: usize = 300;
/// How many ambiguous rows the review screen shows at once. The first match is kept for any row
/// past this, which the review says rather than silently leaving the rest out.
const REVIEW_SHOWN: usize = 100;

/// Where the track list comes from. The three are equal paths; none needs an account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Link,
    File,
    Paste,
}

impl Source {
    fn label(self) -> SharedString {
        match self {
            Self::Link => t!("import-source-link"),
            Self::File => t!("import-source-file"),
            Self::Paste => t!("import-source-paste"),
        }
    }
}

pub(crate) struct SpotifyImport {
    input: Entity<Input>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    open: bool,
    show_unmatched: bool,
    source: Source,
    file: Option<PathBuf>,
    manual: Option<SharedString>,
    manual_tracks: usize,
    note: Option<SharedString>,
    picking: Option<Task<()>>,
}

struct Installed(Entity<SpotifyImport>);

impl Global for Installed {}

impl SpotifyImport {
    pub fn entity(cx: &mut App) -> Entity<Self> {
        if cx.try_global::<Installed>().is_none() {
            let dialog = cx.new(|cx| {
                let dialog = Self {
                    input: cx.new(|cx| Input::new("import-placeholder", cx)),
                    focus: cx.focus_handle(),
                    restore: None,
                    open: false,
                    show_unmatched: false,
                    source: Source::Link,
                    file: None,
                    manual: None,
                    manual_tracks: 0,
                    note: None,
                    picking: None,
                };
                // The import entity notifies as it moves, and a result that lands while the
                // dialog is closed is ready when it opens again.
                let imports = Soundy::global(cx).imports.clone();
                cx.observe(&imports, |_, _, cx| cx.notify()).detach();
                dialog
            });
            cx.set_global(Installed(dialog));
        }
        cx.global::<Installed>().0.clone()
    }

    pub fn open(window: &mut Window, cx: &mut App) {
        let dialog = Self::entity(cx);
        dialog.update(cx, |this, cx| this.show(window, cx));
    }

    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let imports = Soundy::global(cx).imports.clone();
        if !imports.read(cx).active() && imports.read(cx).stage() == ImportStage::Idle {
            self.clear(cx);
        }
        self.restore = window.focused(cx);
        self.open = true;
        if imports.read(cx).active() {
            window.focus(&self.focus, cx);
        } else {
            self.focus_field(window, cx);
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.show_unmatched = false;
        self.picking = None;
        let imports = Soundy::global(cx).imports.clone();
        if !imports.read(cx).active() {
            imports.update(cx, |import, cx| import.reset(cx));
        }
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let imports = Soundy::global(cx).imports.clone();
        imports.update(cx, |import, cx| import.reset(cx));
        self.input.update(cx, |input, cx| input.set_text("", cx));
        self.file = None;
        self.manual = None;
        self.manual_tracks = 0;
        self.note = None;
        self.show_unmatched = false;
    }

    fn focus_field(&mut self, window: &mut Window, cx: &mut App) {
        match self.source {
            Source::Link => self.input.update(cx, |input, cx| input.focus(window, cx)),
            Source::File | Source::Paste => window.focus(&self.focus, cx),
        }
    }

    /// Hands the field back after a result, with the choices still on screen.
    fn again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear(cx);
        self.source = Source::Link;
        self.focus_field(window, cx);
        cx.notify();
    }

    fn select(&mut self, source: Source, window: &mut Window, cx: &mut Context<Self>) {
        let imports = Soundy::global(cx).imports.clone();
        if imports.read(cx).active() {
            return;
        }
        if imports.read(cx).stage() == ImportStage::Failed {
            imports.update(cx, |import, cx| import.reset(cx));
        }
        self.source = source;
        self.note = None;
        self.focus_field(window, cx);
        cx.notify();
    }

    /// Asks the desktop for one playlist file. The parser is chosen from the extension, so the
    /// dialog accepts M3U, M3U8, CSV and plain text.
    fn choose_file(&mut self, cx: &mut Context<Self>) {
        if self.picking.is_some() {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("import-file-prompt")),
        });
        self.note = None;
        self.picking = Some(cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Err(error)) => {
                    log::warn!("import: cannot open the file dialog: {error:#}");
                    None
                }
                _ => None,
            };
            this.update(cx, |this, cx| {
                if let Some(path) = picked {
                    this.file = Some(path);
                }
                this.picking = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Reads the clipboard as a pasted track list. The list is parsed here only to say how many
    /// tracks it holds; the import itself parses it again off the UI thread.
    fn paste_list(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.note = Some(t!("import-paste-empty"));
            cx.notify();
            return;
        };
        if text.trim().is_empty() {
            self.note = Some(t!("import-paste-empty"));
            cx.notify();
            return;
        }
        if text.len() > MAX_IMPORT_BYTES {
            self.note = Some(t!("import-file-too-large"));
            cx.notify();
            return;
        }
        let count = parse_manual(&text, "clipboard")
            .map(|playlist| playlist.entries.len())
            .unwrap_or(0);
        if count == 0 {
            self.note = Some(t!("import-paste-empty"));
            cx.notify();
            return;
        }
        self.manual_tracks = count;
        self.manual = Some(SharedString::from(text));
        self.note = None;
        cx.notify();
    }

    fn ready(&self, cx: &App) -> bool {
        match self.source {
            Source::Link => {
                let text = self.input.read(cx).text().trim().to_owned();
                !text.is_empty() && parse_spotify_link(&text).is_ok()
            }
            Source::File => self.file.is_some(),
            Source::Paste => self.manual.is_some(),
        }
    }

    /// The link in the field, when it already looks like a Spotify address but is not a
    /// playlist link this build accepts. A field someone is still typing stays quiet.
    fn link_problem(&self, cx: &App) -> Option<SharedString> {
        let text = self.input.read(cx).text().trim().to_owned();
        let looks_spotify = text.contains("spotify.com")
            || text.starts_with("spotify:")
            || (text.len() == 22 && text.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        if !looks_spotify {
            return None;
        }
        parse_spotify_link(&text)
            .err()
            .map(|_| t!("import-invalid-link"))
    }

    /// Puts the clipboard's link in the field without importing it, so it can be checked first.
    fn paste_link(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.note = Some(t!("import-link-empty"));
            cx.notify();
            return;
        };
        let text = text.trim();
        if text.is_empty() {
            self.note = Some(t!("import-link-empty"));
            cx.notify();
            return;
        }
        self.input
            .update(cx, |input, cx| input.set_text(text.to_owned(), cx));
        self.note = None;
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let imports = Soundy::global(cx).imports.clone();
        if imports.read(cx).active() {
            return;
        }
        match self.source {
            Source::Link => {
                let text = self.input.read(cx).text().trim().to_owned();
                if text.is_empty() {
                    return;
                }
                imports.update(cx, |import, cx| import.start_link(&text, cx));
            }
            Source::File => match self.file.clone() {
                Some(path) => imports.update(cx, |import, cx| import.start_file(path, cx)),
                None => {
                    self.choose_file(cx);
                    return;
                }
            },
            Source::Paste => match self.manual.clone() {
                Some(text) => imports.update(cx, |import, cx| import.start_manual(&text, cx)),
                None => {
                    self.paste_list(cx);
                    return;
                }
            },
        }
        // A link that failed before any request keeps the field focused and the message on
        // screen; one that started keeps focus so Escape still closes the dialog.
        if !imports.read(cx).active() {
            self.focus_field(window, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn open_playlist(
        &mut self,
        report: &ImportReport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let destination = Destination::Playlist(report.playlist_id.clone().into());
        self.close(window, cx);
        navigate(destination, cx);
    }

    fn sources(&self, cx: &mut Context<Self>) -> TabBar {
        TabBar::new("import-sources")
            .name(t!("import-sources-name"))
            .items([Source::Link, Source::File, Source::Paste].map(|source| {
                Button::new(match source {
                    Source::Link => "import-source-link",
                    Source::File => "import-source-file",
                    Source::Paste => "import-source-paste",
                })
                .label(source.label())
                .small()
                .ghost()
                .selected(self.source == source)
                .on_click(cx.listener(move |this, _, window, cx| this.select(source, window, cx)))
            }))
    }

    /// The choice and its explanation, with the last failure under it.
    fn field(&self, failure: Option<ImportFailure>, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let body = match self.source {
            Source::Link => self.link_field(cx).into_any_element(),
            Source::File => self.file_field(cx).into_any_element(),
            Source::Paste => self.paste_field(cx).into_any_element(),
        };
        let spotify = Soundy::global(cx)
            .session
            .read(cx)
            .client()
            .and_then(|client| client.spotify_web())
            .is_some();
        let hint = match (self.source, spotify) {
            (Source::Link, true) => t!("import-link-hint-signed-in"),
            (Source::Link, false) => t!("import-link-hint"),
            (Source::File, _) => t!("import-file-hint"),
            (Source::Paste, _) => t!("import-paste-hint"),
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.sources(cx))
            .child(body)
            .when_some(failure, |column, failure| {
                column.child(self.failure(failure, cx))
            })
            .child(
                div()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground.opacity(0.8))
                    .child(hint),
            )
    }

    fn link_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let form = Form::new("import-link-form").field(Field::new(&self.input, cx));
        let problem = self.link_problem(cx);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(theme.text(Text::Body))
                    .text_color(theme.muted_foreground)
                    .child(t!("import-link-detail")),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(form))
                    .child(
                        Button::new("import-link-paste")
                            .outline()
                            .icon("icons/clipboard-paste.svg")
                            .label(t!("import-link-paste"))
                            .on_click(cx.listener(|this, _, _, cx| this.paste_link(cx))),
                    ),
            )
            .when_some(problem, |column, message| {
                column.child(
                    div()
                        .text_size(theme.text(Text::Small))
                        .text_color(theme.danger_foreground)
                        .child(message),
                )
            })
    }

    fn file_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let name = self
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| SharedString::from(name.to_string_lossy().into_owned()));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(theme.text(Text::Body))
                    .text_color(theme.muted_foreground)
                    .child(t!("import-file-detail")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Button::new("import-choose-file")
                            .outline()
                            .icon("icons/folder-open.svg")
                            .label(t!("import-file-choose"))
                            .disabled(self.picking.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.choose_file(cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(theme.text(Text::Body))
                                    .child(name.unwrap_or_else(|| t!("import-file-none"))),
                            )
                            .when_some(self.note.clone(), |column, note| {
                                column.child(
                                    div()
                                        .text_size(theme.text(Text::Small))
                                        .text_color(theme.danger_foreground)
                                        .child(note),
                                )
                            }),
                    ),
            )
    }

    fn paste_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(theme.text(Text::Body))
                    .text_color(theme.muted_foreground)
                    .child(t!("import-paste-detail")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Button::new("import-paste-list")
                            .outline()
                            .icon("icons/clipboard-paste.svg")
                            .label(t!("import-paste-action"))
                            .on_click(cx.listener(|this, _, _, cx| this.paste_list(cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_size(theme.text(Text::Body)).child(
                                match self.manual.is_some() {
                                    true => {
                                        t!("import-paste-count", count = self.manual_tracks as i64)
                                    }
                                    false => t!("import-paste-none"),
                                },
                            ))
                            .when_some(self.note.clone(), |column, note| {
                                column.child(
                                    div()
                                        .text_size(theme.text(Text::Small))
                                        .text_color(theme.danger_foreground)
                                        .child(note),
                                )
                            }),
                    ),
            )
    }

    fn failure(&self, failure: ImportFailure, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let cancelled = matches!(failure, ImportFailure::Cancelled);
        let (message, fallback) = match failure {
            ImportFailure::InvalidLink => (t!("import-invalid-link"), false),
            ImportFailure::TracklistUnavailable | ImportFailure::SpotifyNotConnected => {
                (t!("import-tracklist-unavailable"), true)
            }
            ImportFailure::PublicPageChanged => (t!("import-public-page-changed"), true),
            ImportFailure::Unauthorized => (t!("import-unauthorized"), true),
            ImportFailure::Forbidden => (t!("import-forbidden"), true),
            ImportFailure::NotFound => (t!("import-not-found"), true),
            ImportFailure::RateLimited { seconds } => {
                (t!("import-rate-limited", seconds = seconds as i64), true)
            }
            ImportFailure::Cancelled => (t!("import-cancelled"), false),
            ImportFailure::EmptySource => (t!("import-empty-source"), false),
            ImportFailure::UnsupportedFile(name) => {
                (t!("import-unsupported-file", name = name), false)
            }
            ImportFailure::FileTooLarge => (t!("import-file-too-large"), false),
            ImportFailure::File(reason) => (SharedString::from(reason), false),
            ImportFailure::Network(reason) => (t!("import-network", reason = reason), true),
            ImportFailure::Other(reason) => (SharedString::from(reason), false),
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .items_start()
            .child(
                Notice::new(
                    match cancelled {
                        true => t!("import-cancelled-title"),
                        false => t!("import-failed"),
                    },
                    message,
                )
                .failed()
                .spoken("import-failure"),
            )
            .when(fallback && self.source == Source::Link, |column| {
                column
                    .child(
                        div()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(t!("import-fallback-detail")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("import-fallback-file")
                                    .outline()
                                    .small()
                                    .icon("icons/folder-open.svg")
                                    .label(t!("import-fallback-file"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.select(Source::File, window, cx);
                                        this.choose_file(cx);
                                    })),
                            )
                            .child(
                                Button::new("import-fallback-paste")
                                    .outline()
                                    .small()
                                    .icon("icons/clipboard-paste.svg")
                                    .label(t!("import-fallback-paste"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.select(Source::Paste, window, cx);
                                        this.paste_list(cx);
                                    })),
                            ),
                    )
            })
    }

    fn progress(
        &self,
        stage: ImportStage,
        read: usize,
        total: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let message = match stage {
            ImportStage::Fetching => match self.source {
                Source::Link => t!("import-progress-link"),
                Source::File => t!("import-progress-file"),
                Source::Paste => t!("import-progress-paste"),
            },
            ImportStage::Matching if total > 0 => {
                t!(
                    "import-progress-matching",
                    read = read as i64,
                    total = total as i64
                )
            }
            ImportStage::Matching => t!("import-progress-matching-unknown"),
            ImportStage::Enriching if total > 0 => {
                t!(
                    "import-progress-enriching",
                    read = read as i64,
                    total = total as i64
                )
            }
            ImportStage::Enriching => t!("import-progress-enriching-unknown"),
            ImportStage::Saving => t!("import-progress-saving"),
            _ => t!("import-progress-link"),
        };
        let metadata = match stage {
            ImportStage::Enriching => t!("import-metadata-artwork"),
            _ => match self.source {
                Source::Link => t!("import-metadata-spotify"),
                Source::File => t!("import-metadata-file"),
                Source::Paste => t!("import-metadata-paste"),
            },
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(theme.text(Text::Body))
                    .child(Spinner::new("import-spinner"))
                    .child(message),
            )
            .when(total > 0, |column| {
                let shown = (read.min(total) as f32) / (total as f32);
                column
                    .child(
                        div()
                            .h(px(3.))
                            .w_full()
                            .rounded_full()
                            .bg(theme.muted)
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(theme.primary)
                                    .w(gpui::relative(shown)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(metadata)
                            .child(t!("import-progress-detail")),
                    )
            })
            .child(
                div()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground)
                    .child(t!("import-cancel-hint")),
            )
    }

    fn source_label(&self, source: &str) -> SharedString {
        match source {
            "spotify" => t!("import-source-spotify"),
            "csv" => t!("import-source-csv"),
            "m3u" => t!("import-source-m3u"),
            "manual" => t!("import-source-manual"),
            _ => t!("import-source-other"),
        }
    }

    fn result(&mut self, report: &ImportReport, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let playable = report.playable();
        let mut counts = vec![
            (
                t!("spotify-import-playable", count = playable as i64),
                theme.primary,
            ),
            (
                t!("spotify-import-personal", count = report.personal as i64),
                theme.foreground,
            ),
            (
                t!("spotify-import-preview", count = report.preview as i64),
                theme.muted_foreground,
            ),
            (
                t!("import-external", count = report.external as i64),
                theme.muted_foreground,
            ),
            (
                t!(
                    "spotify-import-unavailable",
                    count = report.unavailable as i64
                ),
                theme.danger_foreground.opacity(0.85),
            ),
        ];
        if report.duplicates > 0 {
            counts.push((
                t!("import-duplicates", count = report.duplicates as i64),
                theme.muted_foreground,
            ));
        }
        if report.artwork > 0 {
            counts.push((
                t!("import-artwork-count", count = report.artwork as i64),
                theme.muted_foreground,
            ));
        }
        if report.collage {
            counts.push((t!("import-collage"), theme.primary));
        }
        if report.skipped > 0 {
            counts.push((
                t!("import-skipped", count = report.skipped as i64),
                theme.danger_foreground.opacity(0.85),
            ));
        }
        let imported_at = crate::shared::cells::stamp(Some(report.imported_at));
        let source_url = report.source_url.clone();
        let name = report.name.clone();
        let description = report.description.clone();
        let label = self.source_label(&report.source);
        let unmatched = report.unmatched.len();
        let shown: Vec<_> = report
            .unmatched
            .iter()
            .take(UNMATCHED_SHOWN)
            .cloned()
            .collect();
        let open = self.show_unmatched;

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .gap(theme.metrics.pad)
                    .child(
                        Artwork::new(report.cover.clone())
                            .size(theme.metrics.cover)
                            .corner_radius(theme.radius)
                            .fallback("icons/list-music.svg")
                            .flex_none(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(theme.text(Text::Heading))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(t!(
                                        "import-result-detail",
                                        total = report.total as i64,
                                        date = &imported_at
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(t!("import-result-source", source = &label)),
                            )
                            .when(!description.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_size(theme.text(Text::Small))
                                        .text_color(theme.muted_foreground)
                                        .child(description.clone()),
                                )
                            })
                            .when(!source_url.is_empty(), |column| {
                                let source = source_url.clone();
                                column.child(
                                    Button::new("import-source-open")
                                        .ghost()
                                        .small()
                                        .icon("icons/external-link.svg")
                                        .label(t!("import-open-source"))
                                        .on_click(move |_, _, cx| cx.open_url(&source)),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(counts.into_iter().map(|(label, tint)| {
                        div()
                            .rounded_full()
                            .px_2()
                            .py(px(2.))
                            .bg(theme.muted)
                            .text_size(theme.text(Text::Label))
                            .text_color(tint)
                            .child(label)
                    })),
            )
            .when(report.limited, |column| {
                column.child(
                    Notice::new(t!("import-limited-title"), t!("import-limited"))
                        .spoken("import-limited"),
                )
            })
            .child(
                Button::new("import-unmatched")
                    .ghost()
                    .small()
                    .icon(match open {
                        true => "icons/chevron-up.svg",
                        false => "icons/chevron-down.svg",
                    })
                    .label(match open {
                        true => t!("spotify-import-unmatched-hide"),
                        false => t!("spotify-import-unmatched", count = unmatched as i64),
                    })
                    .disabled(unmatched == 0)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_unmatched = !this.show_unmatched;
                        cx.notify();
                    })),
            )
            .when(open && unmatched > 0, |column| {
                column
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .children(shown.into_iter().enumerate().map(|(index, entry)| {
                                let status = entry.status;
                                let url = entry
                                    .external_url
                                    .clone()
                                    .or_else(|| entry.source_url.clone());
                                let link = entry.external_url.clone().map(SharedString::from);
                                div()
                                    .id(("import-unmatched-row", index))
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .when_some(url, |row, url| {
                                        row.cursor_pointer().on_click(move |_, _, cx| {
                                            cx.open_url(&url);
                                        })
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_w_0()
                                            .child(div().truncate().child(entry.title))
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_size(theme.text(Text::Small))
                                                    .text_color(theme.muted_foreground)
                                                    .child(entry.artists),
                                            ),
                                    )
                                    .child(crate::shared::cells::import_status(
                                        index, status, link, cx,
                                    ))
                            })),
                    )
                    .when(unmatched > UNMATCHED_SHOWN, |column| {
                        column.child(
                            div()
                                .text_size(theme.text(Text::Small))
                                .text_color(theme.muted_foreground)
                                .child(t!(
                                    "spotify-import-unmatched-more",
                                    shown = UNMATCHED_SHOWN as i64,
                                    total = unmatched as i64
                                )),
                        )
                    })
            })
            .child(
                div()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground)
                    .child(t!("import-compliance")),
            )
    }

    /// The songs that matched more than one playable recording, each with the candidates to
    /// choose between. Every choice is a real track the active provider can play.
    fn review(&mut self, review: &Review, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let shown: Vec<ReviewRow> = review.rows.iter().take(REVIEW_SHOWN).cloned().collect();
        let count = shown.len();
        let total = review.total;
        let mut rows = Vec::with_capacity(count);
        for row in shown {
            rows.push(self.candidate_row(row, cx).into_any_element());
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(theme.text(Text::Heading))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(t!("import-review-title")),
                    )
                    .child(
                        div()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(t!("import-review-detail", count = total as i64)),
                    ),
            )
            .child(div().flex().flex_col().children(rows))
            .when(total > count, |column| {
                column.child(
                    div()
                        .text_size(theme.text(Text::Small))
                        .text_color(theme.muted_foreground)
                        .child(t!(
                            "import-review-hidden",
                            shown = count as i64,
                            total = total as i64
                        )),
                )
            })
            .child(
                div()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground)
                    .child(t!("import-review-note")),
            )
    }

    /// One ambiguous song: what the source said, then one button per playable candidate.
    fn candidate_row(&self, row: ReviewRow, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let ReviewRow {
            position,
            title,
            artists,
            album,
            duration,
            candidates,
            chosen,
        } = row;
        let source_title = title.clone();
        let detail = match album.is_empty() {
            true => format!("{artists} · {}", clock(duration)),
            false => format!("{artists} · {album} · {}", clock(duration)),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().truncate().child(title))
                    .child(
                        div()
                            .truncate()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(detail),
                    ),
            )
            .child(div().flex().flex_wrap().gap_2().children(
                candidates.into_iter().enumerate().map(|(index, track)| {
                    let length = clock(track.duration);
                    // A provider sometimes reports no album for a search result. Showing
                    // "· 3:15" for those would leave the listener guessing, so the recording's
                    // own title stands in for the missing album.
                    let label = match (track.name == source_title, track.album.is_empty()) {
                        (_, true) => t!(
                            "import-review-candidate-title",
                            title = &track.name,
                            duration = &length
                        ),
                        (true, false) => t!(
                            "import-review-candidate",
                            album = &track.album,
                            duration = &length
                        ),
                        (false, false) => t!(
                            "import-review-candidate-named",
                            title = &track.name,
                            album = &track.album,
                            duration = &length
                        ),
                    };
                    Button::new(format!("import-review-choice-{position}-{index}"))
                        .ghost()
                        .small()
                        .selected(chosen == index)
                        .label(label)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            Soundy::global(cx)
                                .imports
                                .clone()
                                .update(cx, |import, cx| import.choose(position, index, cx));
                        }))
                }),
            ))
    }
}

impl Render for SpotifyImport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }

        let theme = *cx.theme();
        let imports = Soundy::global(cx).imports.clone();
        let (stage, active, report, failure, review, read, total) = {
            let import = imports.read(cx);
            (
                import.stage(),
                import.active(),
                import.report().cloned(),
                import.failure().cloned(),
                import.review().cloned(),
                import.read(),
                import.total(),
            )
        };
        let idle = matches!(stage, ImportStage::Idle | ImportStage::Failed);

        let body = match (active, report.clone(), review.clone()) {
            (true, _, _) => self.progress(stage, read, total, cx).into_any_element(),
            (false, Some(report), _) => self.result(&report, cx).into_any_element(),
            (false, None, Some(review)) => self.review(&review, cx).into_any_element(),
            (false, None, None) => self.field(failure, cx).into_any_element(),
        };

        let mut actions: Vec<Button> = Vec::new();
        if active {
            actions.push(
                Button::new("import-cancel")
                    .ghost()
                    .label(t!("spotify-import-cancel"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        let imports = Soundy::global(cx).imports.clone();
                        imports.update(cx, |import, cx| import.cancel(cx));
                    })),
            );
        }
        if idle || active || review.is_some() {
            actions.push(
                Button::new("import-close")
                    .ghost()
                    .label(t!("spotify-import-close"))
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            );
        }
        if review.is_some() {
            actions.push(
                Button::new("import-review-back")
                    .ghost()
                    .label(t!("import-review-back"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        Soundy::global(cx)
                            .imports
                            .clone()
                            .update(cx, |import, cx| import.discard_review(cx));
                    })),
            );
        }
        if report.is_some() && !active {
            actions.push(
                Button::new("import-again")
                    .ghost()
                    .label(t!("spotify-import-another"))
                    .on_click(cx.listener(|this, _, window, cx| this.again(window, cx))),
            );
        }
        if !active && imports.read(cx).can_retry_artwork() {
            actions.push(
                Button::new("import-retry-artwork")
                    .ghost()
                    .icon("icons/refresh-cw.svg")
                    .label(t!("import-retry-artwork"))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        imports.update(cx, |import, cx| import.retry_artwork(cx));
                    })),
            );
        }
        if let Some(report) = report.filter(|_| !active) {
            actions.push(
                Button::new("import-open")
                    .primary()
                    .icon("icons/list-music.svg")
                    .label(t!("spotify-import-open-playlist"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_playlist(&report, window, cx);
                    })),
            );
        }
        if review.is_some() {
            actions.push(
                Button::new("import-review-confirm")
                    .primary()
                    .icon("icons/download.svg")
                    .label(t!("import-review-confirm"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        Soundy::global(cx)
                            .imports
                            .clone()
                            .update(cx, |import, cx| import.confirm(cx));
                    })),
            );
        }
        if idle {
            actions.push(
                Button::new("import-submit")
                    .primary()
                    .icon("icons/download.svg")
                    .label(t!("import-action"))
                    .disabled(!self.ready(cx))
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );
        }

        let mut modal = Modal::new("import", t!("import-title"))
            .w(theme.metrics.cover * 3.2)
            .detail(t!("import-subtitle"))
            .close_button()
            .trailing(
                svg()
                    .path(icons::path("icons/list-music.svg"))
                    .size(px(22.))
                    .text_color(theme.foreground.opacity(0.7)),
            )
            .child(body);
        for action in actions {
            modal = modal.action(action);
        }

        div()
            .absolute()
            .inset_0()
            .key_context(ui::FORM_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                cx.stop_propagation();
                this.close(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                let imports = Soundy::global(cx).imports.clone();
                if imports.read(cx).stage() == ImportStage::Review {
                    imports.update(cx, |import, cx| import.confirm(cx));
                    return;
                }
                this.submit(window, cx);
            }))
            .child(modal.on_dismiss(cx.listener(|this, _, window, cx| this.close(window, cx))))
            .into_any_element()
    }
}
