//! The dialog that imports a Spotify playlist into the library.
//!
//! The dialog owns the field and the screen; the work lives in `state::Import`. Opening it
//! makes no request: the fetch starts when Import is pressed, and everything the network and
//! the database do runs away from the UI thread. While a fetch runs the dialog reads the
//! counters the import publishes, so it redraws only when those counters move.

use gpui::prelude::*;
use gpui::{App, Context, Entity, FocusHandle, Global, Render, SharedString, Window, div, px, svg};
use i18n::t;
use music::import::ImportFailure;
use router::{Destination, SettingsPage, navigate};
use state::{ImportReport, ImportStage, Sonora};
use ui::{
    ActiveTheme as _, Artwork, Button, Dismiss, Field, Form, Input, Modal, Notice, Spinner, Submit,
    Text,
};

/// How many unmatched rows the result lists before it says the rest are not shown. A playlist
/// of thousands is still readable; the modal scrolls through the list.
const UNMATCHED_SHOWN: usize = 300;

pub(crate) struct SpotifyImport {
    input: Entity<Input>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    open: bool,
    show_unmatched: bool,
}

struct Installed(Entity<SpotifyImport>);

impl Global for Installed {}

impl SpotifyImport {
    pub fn entity(cx: &mut App) -> Entity<Self> {
        if cx.try_global::<Installed>().is_none() {
            let dialog = cx.new(|cx| {
                let dialog = Self {
                    input: cx.new(|cx| Input::new("spotify-import-placeholder", cx)),
                    focus: cx.focus_handle(),
                    restore: None,
                    open: false,
                    show_unmatched: false,
                };
                // The import entity notifies as it moves, and a result that lands while the
                // dialog is closed is ready when it opens again.
                let imports = Sonora::global(cx).imports.clone();
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
        let imports = Sonora::global(cx).imports.clone();
        if !imports.read(cx).active() {
            imports.update(cx, |import, cx| import.reset(cx));
            self.input.update(cx, |input, cx| input.set_text("", cx));
            self.show_unmatched = false;
        }
        self.restore = window.focused(cx);
        self.open = true;
        if imports.read(cx).active() {
            window.focus(&self.focus, cx);
        } else {
            self.input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.show_unmatched = false;
        let imports = Sonora::global(cx).imports.clone();
        if !imports.read(cx).active() {
            imports.update(cx, |import, cx| import.reset(cx));
        }
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let imports = Sonora::global(cx).imports.clone();
        if imports.read(cx).active() {
            return;
        }
        let text = self.input.read(cx).text().trim().to_owned();
        imports.update(cx, |import, cx| import.start(&text, cx));
        // A link that failed before any request leaves the field focused and the message on
        // screen; one that started keeps focus so Escape still closes the dialog.
        if !imports.read(cx).active() {
            self.input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let imports = Sonora::global(cx).imports.clone();
        imports.update(cx, |import, cx| import.reset(cx));
        self.input.update(cx, |input, cx| input.set_text("", cx));
        self.show_unmatched = false;
        self.input.update(cx, |input, cx| input.focus(window, cx));
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

    /// The field and the explanation, with the last failure under it.
    fn field(&self, failure: Option<ImportFailure>, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let form = Form::new("spotify-import-form").field(Field::new(&self.input, cx));

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(theme.text(Text::Body))
                    .text_color(theme.muted_foreground)
                    .child(t!("spotify-import-detail")),
            )
            .child(form)
            .when_some(failure, |column, failure| {
                column.child(self.failure(failure, cx))
            })
            .child(
                div()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground.opacity(0.8))
                    .child(t!("spotify-import-accounts-hint")),
            )
    }

    fn failure(&self, failure: ImportFailure, cx: &mut Context<Self>) -> impl IntoElement {
        let (message, sign_in) = match failure {
            ImportFailure::InvalidLink => (t!("spotify-import-invalid-link"), false),
            ImportFailure::SpotifyNotConnected => (t!("spotify-import-not-connected"), true),
            ImportFailure::Unauthorized => (t!("spotify-import-unauthorized"), true),
            ImportFailure::Forbidden => (t!("spotify-import-forbidden"), false),
            ImportFailure::NotFound => (t!("spotify-import-not-found"), false),
            ImportFailure::RateLimited { seconds } => (
                t!("spotify-import-rate-limited", seconds = seconds as i64),
                false,
            ),
            ImportFailure::Cancelled => (t!("spotify-import-cancelled"), false),
            ImportFailure::Network(reason) => {
                (t!("spotify-import-network", reason = reason), false)
            }
            ImportFailure::Other(reason) => (SharedString::from(reason), false),
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .items_start()
            .child(
                Notice::new(t!("spotify-import-failed"), message)
                    .failed()
                    .spoken("spotify-import-failure"),
            )
            .when(sign_in, |column| {
                column.child(
                    Button::new("spotify-import-sign-in")
                        .outline()
                        .small()
                        .icon("icons/user-round.svg")
                        .label(t!("spotify-import-sign-in"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close(window, cx);
                            navigate(Destination::Settings(SettingsPage::Providers), cx);
                        })),
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
            ImportStage::Fetching if total > 0 => {
                t!(
                    "spotify-import-progress",
                    read = read as i64,
                    total = total as i64
                )
            }
            ImportStage::Fetching => t!("spotify-import-reading"),
            ImportStage::Matching => t!("spotify-import-matching"),
            ImportStage::Saving => t!("spotify-import-saving"),
            _ => t!("spotify-import-reading"),
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
                    .child(Spinner::new("spotify-import-spinner"))
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
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(t!("spotify-import-progress-detail")),
                    )
            })
            .child(
                div()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground)
                    .child(t!("spotify-import-cancel-hint")),
            )
    }

    fn result(&mut self, report: &ImportReport, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let playable = report.playable();
        let counts = [
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
                t!("spotify-import-external", count = report.external as i64),
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
        let imported_at = crate::shared::cells::stamp(Some(report.imported_at));
        let source = report.source_url.clone();
        let name = report.name.clone();
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
                            .fallback("icons/spotify.svg")
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
                                        "spotify-import-result-detail",
                                        total = report.total as i64,
                                        date = &imported_at
                                    )),
                            )
                            .child(
                                Button::new("spotify-import-source")
                                    .ghost()
                                    .small()
                                    .icon("icons/external-link.svg")
                                    .label(t!("spotify-import-open-source"))
                                    .on_click(move |_, _, cx| cx.open_url(&source)),
                            ),
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
            .child(
                Button::new("spotify-import-unmatched")
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
                                let url = entry.source_url.clone();
                                div()
                                    .id(("spotify-import-unmatched-row", index))
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
                                    .child(crate::shared::cells::import_status(index, status, cx))
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
                    .child(t!("spotify-import-compliance")),
            )
    }
}

impl Render for SpotifyImport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }

        let theme = *cx.theme();
        let imports = Sonora::global(cx).imports.clone();
        let (stage, active, report, failure, read, total) = {
            let import = imports.read(cx);
            (
                import.stage(),
                import.active(),
                import.report().cloned(),
                import.failure().cloned(),
                import.read(),
                import.total(),
            )
        };
        let idle = matches!(stage, ImportStage::Idle | ImportStage::Failed);

        let body = match (active, report.clone()) {
            (true, _) => self.progress(stage, read, total, cx).into_any_element(),
            (false, Some(report)) => self.result(&report, cx).into_any_element(),
            (false, None) => self.field(failure, cx).into_any_element(),
        };

        let mut actions: Vec<Button> = Vec::new();
        if active {
            actions.push(
                Button::new("spotify-import-cancel")
                    .ghost()
                    .label(t!("spotify-import-cancel"))
                    .on_click(cx.listener(|_, _, _, cx| {
                        let imports = Sonora::global(cx).imports.clone();
                        imports.update(cx, |import, cx| import.cancel(cx));
                    })),
            );
        }
        if idle || active {
            actions.push(
                Button::new("spotify-import-close")
                    .ghost()
                    .label(t!("spotify-import-close"))
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            );
        }
        if report.is_some() && !active {
            actions.push(
                Button::new("spotify-import-again")
                    .ghost()
                    .label(t!("spotify-import-another"))
                    .on_click(cx.listener(|this, _, window, cx| this.again(window, cx))),
            );
        }
        if let Some(report) = report.filter(|_| !active) {
            actions.push(
                Button::new("spotify-import-open")
                    .primary()
                    .icon("icons/list-music.svg")
                    .label(t!("spotify-import-open-playlist"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_playlist(&report, window, cx);
                    })),
            );
        }
        if idle {
            actions.push(
                Button::new("spotify-import-submit")
                    .primary()
                    .icon("icons/download.svg")
                    .label(t!("spotify-import-action"))
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );
        }

        let mut modal = Modal::new("spotify-import", t!("spotify-import-title"))
            .w(theme.metrics.cover * 3.)
            .detail(t!("spotify-import-subtitle"))
            .close_button()
            .trailing(
                svg()
                    .path(icons::path("icons/spotify.svg"))
                    .size(px(22.))
                    .text_color(crate::shared::provider_brand("spotify", cx)),
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
                this.submit(window, cx);
            }))
            .child(modal.on_dismiss(cx.listener(|this, _, window, cx| this.close(window, cx))))
            .into_any_element()
    }
}
