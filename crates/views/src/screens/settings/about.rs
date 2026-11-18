use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, px, svg};
use i18n::t;
use ui::{ActiveTheme as _, Artwork, Glowing as _, Text};

use super::rows::outward;
use super::{Group, SettingsView, Slot, group_heading};

/// How large the app icon stands, in covers.
const MARK_TILE: f32 = 0.5;
/// The app icon as this platform shows it, registered by the binary's asset source.
const APP_ICON: &str = "brand/soundy.svg";
/// The developer's GitHub avatar, kept beside the binary so the card has a face with no
/// network. The copy GitHub serves is laid over it once it arrives, and whenever it fails the
/// bundled one stays legible underneath.
const PROFILE_AVATAR: &str = "brand/ganddtn40.jpg";
const DEVELOPER_NAME: &str = "ℨ𝔶𝔯𝔬𝔙𝔞𝔵";
const DEVELOPER_LOGIN: &str = "@ganddtn40";
const DEVELOPER_URL: &str = "https://github.com/ganddtn40";
/// The avatar GitHub serves for the login. A week number in the query makes the image cache
/// miss once a week, which refreshes it without polling and without a request on every launch.
const DEVELOPER_AVATAR: &str = "https://github.com/ganddtn40.png?size=200&week=";
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
const SOURCE_URL: &str = "https://github.com/sonorahq/sonora";

/// The avatar URL for this week, so a changed picture reaches the card within a week while the
/// HTTP cache answers every launch in between.
fn developer_avatar() -> String {
    let week = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() / (7 * 24 * 60 * 60))
        .unwrap_or_default();
    format!("{DEVELOPER_AVATAR}{week}")
}

impl SettingsView {
    pub(super) fn about_groups(&self) -> Vec<Group> {
        // This fork has no update feed of its own, so About names the build it is rather
        // than offering to replace itself with an upstream Sonora release.
        vec![Group::untitled().rows([Slot::Version, Slot::Log, Slot::License, Slot::Source])]
    }

    /// The version row, which for a nightly also names the commit it was built from.
    pub(super) fn version_row(&self) -> AnyElement {
        let version: SharedString = match state::commit() {
            Some(commit) => t!(
                "settings-version-commit",
                version = state::version(),
                commit = commit
            ),
            None => state::version().into(),
        };
        self.row(
            t!("settings-version"),
            ui::spoken("settings-version-value", version).into_any_element(),
        )
    }

    /// The row that opens a copy of the current log file in whatever the system reads text
    /// with. It is disabled when the platform names no state or cache folder to log into.
    pub(super) fn log_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = state::log_file();

        outward("open-log", t!("settings-log-open"), None, cx)
            .disabled(path.is_none())
            .on_click(move |_, _, _| {
                let Some(path) = &path else { return };
                if let Err(error) = snapshot(path).and_then(open::that_detached) {
                    log::warn!(
                        "settings: cannot open a copy of {}: {error}",
                        path.display()
                    );
                }
            })
            .into_any_element()
    }

    pub(super) fn license_row(&self, cx: &mut Context<Self>) -> AnyElement {
        outward(
            "license",
            t!("settings-license"),
            Some(t!("settings-license-short")),
            cx,
        )
        .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
        .into_any_element()
    }

    pub(super) fn source_row(&self, cx: &mut Context<Self>) -> AnyElement {
        outward("source", t!("settings-source"), None, cx)
            .on_click(|_, _, cx| cx.open_url(SOURCE_URL))
            .into_any_element()
    }

    /// Opens About with the app icon beside its name, aligned with the settings below. The icon
    /// is the one this platform shows, in its own colours whatever the theme.
    pub(super) fn mark(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let tile = theme.metrics.cover * MARK_TILE;

        div()
            .flex()
            .items_center()
            .gap_5()
            .child(img(APP_ICON).size(tile).flex_none())
            .child(
                div()
                    .id("settings-about-name")
                    .role(gpui::accesskit::Role::Heading)
                    .aria_level(2)
                    .aria_label(t!("settings-about-name"))
                    .text_size(theme.text(Text::Title))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t!("settings-about-name")),
            )
    }

    pub(super) fn notice(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .id("settings-notice")
            .role(gpui::accesskit::Role::Paragraph)
            .aria_label(t!("settings-notice"))
            .text_color(theme.muted_foreground)
            .text_size(theme.text(Text::Small))
            .child(t!("settings-notice"))
    }

    /// The developer behind this build, as one card that opens the GitHub profile. The card is
    /// the only member: the upstream team list belongs to Sonora, not to this fork.
    pub(super) fn team(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let avatar = theme.metrics.cover * 0.72;
        let remote = developer_avatar();

        div()
            .flex()
            .flex_col()
            .child(
                group_heading(t!("settings-developer"), Some("icons/github.svg"), cx)
                    .id("settings-developer-heading")
                    .role(gpui::accesskit::Role::Heading)
                    .aria_level(2)
                    .aria_label(t!("settings-developer")),
            )
            .child(
                div()
                    .id("settings-developer")
                    .role(gpui::accesskit::Role::Link)
                    .aria_label(DEVELOPER_NAME)
                    .aria_description(t!("settings-developer-bio"))
                    .flex()
                    .items_center()
                    .gap_4()
                    .p(theme.metrics.pad * 2.)
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.secondary.opacity(0.35))
                    .shadow_sm()
                    .cursor_pointer()
                    .on_click(|_, _, cx| cx.open_url(DEVELOPER_URL))
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .w(avatar)
                            .h(avatar)
                            .child(img(PROFILE_AVATAR).size(avatar).rounded_full())
                            // The live picture fades in over the bundled one, so the card
                            // never shows a placeholder and never waits on the network.
                            .child(
                                Artwork::new(Some(remote))
                                    .size(avatar)
                                    .circle()
                                    .overlay()
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            ),
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(DEVELOPER_NAME),
                            )
                            .child(
                                div()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(DEVELOPER_LOGIN),
                            )
                            .child(
                                div()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(t!("settings-developer-bio")),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(theme.text(Text::Label))
                            .text_color(theme.muted_foreground)
                            .child(t!("settings-developer-github"))
                            .child(
                                svg()
                                    .path(icons::path("icons/github.svg"))
                                    .size(px(18.))
                                    .text_color(theme.foreground),
                            ),
                    )
                    .glowed(theme.secondary_hover),
            )
    }
}

/// Notepad on Windows refuses a file that Soundy still writes to, so the button opens a copy.
/// `File::open` shares write access, and `fs::copy` documents no share mode.
fn snapshot(log: &Path) -> io::Result<PathBuf> {
    let copy = log.with_file_name("soundy.copy.log");
    io::copy(&mut File::open(log)?, &mut File::create(&copy)?)?;
    Ok(copy)
}
