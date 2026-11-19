use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Pixels, Render, ScrollHandle, SharedString, Window, div, px,
};
use i18n::t;
use state::Genres;
use ui::{
    ActiveTheme as _, Button, Scrollbar, Scroller, Spinner, Vacancy, spoken_eyebrow, spoken_heading,
};

use crate::chrome::Chrome;
use crate::shared::cells;
use crate::shared::shelves;
use crate::shared::trouble;

/// The Discover page: every genre the active provider offers, as the same plates the search
/// page browses. It owns no data of its own, so opening it never costs a second fetch while the
/// search page has already listed the genres.
pub(crate) struct DiscoverView {
    genres: Entity<Genres>,
    scrollbar: Entity<Scrollbar>,
    width: Pixels,
}

impl DiscoverView {
    pub(crate) fn new(genres: Entity<Genres>, cx: &mut Context<Self>) -> Self {
        let id = cx.entity_id();
        cx.observe(&genres, |_, _, cx| cx.notify()).detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        Self {
            genres,
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(id)),
            width: Pixels::ZERO,
        }
    }

    /// Asks for the genres the first time the page opens. A list already loaded is reused.
    pub(crate) fn open(&mut self, cx: &mut Context<Self>) {
        self.genres.update(cx, |genres, cx| genres.load(cx));
        cx.notify();
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.genres.update(cx, |genres, cx| genres.refresh(cx));
        cx.notify();
    }

    pub(crate) fn is_loading(&self, cx: &App) -> bool {
        self.genres.read(cx).is_loading()
    }

    fn header(&self, cx: &App) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(spoken_eyebrow(
                "discover-eyebrow",
                t!("discover-eyebrow"),
                cx,
            ))
            .child(
                spoken_heading("discover-title", t!("discover-title"), 1, cx)
                    .text_size(px(40.))
                    .line_height(px(44.)),
            )
            .child(
                div()
                    .max_w(px(620.))
                    .text_size(theme.text(ui::Text::Body))
                    .line_height(theme.text(ui::Text::Body) * 1.5)
                    .text_color(theme.muted_foreground)
                    .child(t!("discover-subtitle")),
            )
    }

    fn section(&self, cx: &App) -> impl IntoElement {
        let theme = *cx.theme();
        let label: SharedString = t!("discover-browse");

        div()
            .flex()
            .items_center()
            .justify_between()
            .pb_2()
            .child(
                ui::spoken_eyebrow("discover-browse-heading", label.clone(), cx)
                    .role(gpui::accesskit::Role::Heading)
                    .aria_level(2)
                    .aria_label(label),
            )
            .when(self.genres.read(cx).is_loading(), |row| {
                row.child(Spinner::new("discover-loading"))
            })
            .when(!self.genres.read(cx).is_loading(), |row| {
                row.child(
                    div()
                        .text_size(theme.text(ui::Text::Small))
                        .text_color(theme.muted_foreground),
                )
            })
    }

    fn empty(&self, cx: &Context<Self>) -> AnyElement {
        let connected = state::Soundy::global(cx).session.read(cx).authenticated();

        let vacancy = Vacancy::new(t!("discover-empty"))
            .detail(t!("discover-empty-detail"))
            .icon("icons/radio.svg");
        let vacancy = match connected {
            true => vacancy,
            false => vacancy.action(
                Button::new("discover-provider")
                    .outline()
                    .label(t!("settings-provider"))
                    .icon("icons/settings.svg")
                    .on_click(|_, _, cx| {
                        router::navigate(
                            router::Destination::Settings(router::SettingsPage::Providers),
                            cx,
                        )
                    }),
            ),
        };

        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .py(px(48.))
            .child(vacancy.spoken("discover-empty"))
            .into_any_element()
    }

    fn failure(&self, cx: &Context<Self>) -> AnyElement {
        let genres = self.genres.clone();
        let reason = self.genres.read(cx).error().map(str::to_owned);
        let vacancy = trouble::lost(
            "discover-lost",
            t!("discover-lost"),
            reason.as_deref(),
            move |_, _, cx| genres.update(cx, |genres, cx| genres.refresh(cx)),
            cx,
        );

        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .py(px(48.))
            .child(vacancy)
            .into_any_element()
    }
}

impl Render for DiscoverView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let gutter = Chrome::margin(window, cx);
        let pad = theme.metrics.inset;
        let found = self.genres.read(cx).genres();
        let loading = self.genres.read(cx).is_loading();
        let failed = self.genres.read(cx).error().is_some();

        let width = cells::content_width(window, pad * 2., cx);
        if (width - self.width).abs() >= px(0.5) {
            self.width = width;
        }

        let body: AnyElement = if !found.is_empty() {
            shelves::grid("discover-genre", found, width, window, cx)
        } else if loading {
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .py(px(64.))
                .child(Spinner::new("discover-first-load"))
                .into_any_element()
        } else if failed {
            self.failure(cx)
        } else {
            self.empty(cx)
        };

        let page = Scroller::new("discover-page", &self.scrollbar)
            .hold()
            .bottom_inset(Chrome::bottom(cx))
            .px(gutter)
            .pt(pad)
            .pb(pad)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .px(pad)
                    .child(self.header(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .child(self.section(cx))
                            .child(body),
                    ),
            );

        div().size_full().child(page)
    }
}
