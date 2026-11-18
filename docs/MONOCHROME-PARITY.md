# Monochrome parity

This is the honest feature-by-feature record of what Soundy took from Monochrome Desktop, what it
implements natively, and what it deliberately does not have. It is written after reading the
Monochrome source at commit `8b80216` (`src-tauri/tauri.conf.json` version 2.0.19) and after
exercising the Soundy 1.0.0 release build.

Monochrome is a Tauri + React application. Its repository carries **no licence file**, so no code
was copied from it. It served as a product and behaviour reference only; every row below was
reimplemented in Soundy's Rust + GPUI architecture, or is listed as not integrated.

## Status legend

| Status | Meaning |
| --- | --- |
| **Native** | Built into Soundy's own Rust/GPUI code, with Soundy's data model and tests. |
| **Equivalent** | A different design reaches the same listener outcome. |
| **Partial** | Real, usable, but narrower than Monochrome's version; the gap is stated. |
| **Not integrated** | Deliberately absent in this release; reason given. |
| **Blocked** | Requires a Monochrome service, account or protocol that Soundy does not have. |
| **N/A** | Web/browser-only behaviour with no meaning in a native Windows app. |

Soundy has no Tauri, no JavaScript runtime, no embedded web UI and no second player process. One
`soundy.exe` owns the window, playback, library, queue, settings and importer.

## Shell, navigation and layout

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| One-window desktop shell | `src-tauri`, `index.html` | **Native** | GPUI window; `crates/sonora`, `crates/views` |
| Left sidebar navigation | `index.html` sidebar, `js/router.js` | **Native** | `crates/views/src/chrome/sidebar_left.rs`; Home, Search, Discover, Recents, Songs, Albums, Artists, Playlists, Downloads, Favorites, Settings |
| Collapsible sidebar / icon rail | `themeStore.js`, sidebar CSS | **Native** | Sidebar width drag, fold and persisted state in `state::Settings` |
| Hide or show individual nav rows | `Show … in Sidebar` settings | **Native** | Settings → Entries (`crates/views/src/screens/settings/general.rs`); every row is regular and hideable |
| Pinned playlists/albums/artists | `renderPinnedItems`, `pinned` state | **Native** | `crates/state/src/pins.rs`, drag list in `sidebar_left.rs` |
| Right side panel | `js/side-panel.js` | **Partial** | `crates/views/src/chrome/aside.rs` provides Lyrics and Queue, plus Members inside a Sonora Session. Monochrome's Info and Related tabs are not present |
| Fullscreen now-playing view | `js/ui.js` fullscreen renderer | **Native** | `crates/views/src/shells/fullscreen.rs`; large artwork, artwork palette background, transport, modes, Escape |
| Windowed / fullscreen / immersive modes | fullscreen CSS classes | **Native** | `ToggleWindowFullscreen`, `ToggleFullscreen`, fullscreen shell; persisted per session |
| Command palette | `js/commandPalette.js` | **Not integrated** | Destination search covers navigation; there is no fuzzy action palette over every command |
| Theme link / invite flow | none (community site) | **Native** | `crates/views/src/chrome/theme_invite.rs`, `state::Links`; `sonora://theme/` links install with preview |
| Router and back/forward history | `js/router.js` | **Native** | `crates/router`; destination stack and browser-like back/forward |
| Modal sheets and dialogs | app-wide | **Native** | `crates/ui/src/modal.rs`, one GPUI modal layer including the importer |
| Keyboard shortcut system | settings shortcuts | **Native** | `crates/input`, Settings → Keybinds, global media keys |
| Mobile/PWA layout, install prompt | service worker, manifest | **N/A** | Native Windows build; only the desktop layout is shipped |
| Web share target / upload tab | `index.html` upload tab | **N/A** | No Monochrome upload service; local file import is used instead |

## Discovery, search and library

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| Home feed | `renderHomePage` | **Native** | `crates/views/src/screens/home`; Quick picks deck plus provider shelves, with a Discover pointer when the provider sends no shelves |
| "Hot & New" explore tab | `renderExplorePage` | **Native** | Discover page (`screens/discover.rs`) browses every genre the active provider lists |
| Editor's Picks | `renderHomeEditorsPicks` | **Not integrated** | Monochrome's Editors Picks endpoint has no Soundy equivalent; the provider's own home shelves are used instead |
| AOTY section (discover/releases/charts/lists) | `renderAOTY*` | **Not integrated** | Depends on Album of the Year data Monochrome supplies; no public API is wired |
| Unreleased tracker | `renderUnreleased*` | **Not integrated** | Monochrome-specific tracker API; no provider surface |
| Search tracks/albums/artists/playlists | `renderSearchPage` | **Native** | `screens/search.rs`; grouped hits, best match, recent searches |
| Search podcasts tab | `podcasts-api.js` | **Not integrated** | No podcast catalogue provider; see Podcasts below |
| Search upload tab | upload API | **Not integrated** | No Monochrome account/service to upload to |
| Browse by genre | `renderGenrePage` | **Native** | `screens/genre.rs`, shared `Genres` entity also used by Discover |
| Recent listens | `renderRecentPage` | **Native** | `screens/history.rs` (Recents), local playback history |
| Library: liked tracks | `renderLibraryPage` | **Native** | Songs with `filter-favorites`; sidebar Favorites opens it directly |
| Library: saved albums/artists/playlists | `renderLibraryPage` | **Native** | Albums, Artists, Playlists pages of the merged library |
| Library: local files | `renderLocalFiles` | **Native** | Local provider scan, tags, artwork, lyrics |
| Folder browsing | `renderFolderPage` | **Not integrated** | Local files are browsed by song/album/artist/playlist; there is no raw folder tree |
| Playlist import | `js/playlist-importer.js` | **Native, broader** | CSV, M3U/M3U8, pasted list, Spotify link with honest account limits; metadata/artwork enrichment |
| Duplicate and order preservation | import path | **Native** | Positions stored; duplicates counted and kept |
| Community playlists | `community-playlists.js` | **Blocked** | PocketBase community service; no Soundy account or server |
| Playlist generator | `playlist-generator.js` | **Equivalent** | Smart playlists with boolean/order rules (`screens/equalizer`-adjacent state rules); not prompt-generated |
| Smart recommendations | `smart-recommendations.js` | **Partial** | Provider home recommendations and song radio; no local listening-model recommender |
| Song radio / stations | player radio | **Native** | Provider station playback (`Playback::play_radio`), Queue radio toggle where the provider supports it |
| Mix pages | `renderMixPage` | **Partial** | Provider playlist/mix rows render as ordinary playlists; no dedicated mix page |
| Uploaded tracks | upload API | **Not integrated** | No Monochrome upload backend |
| Podcasts browse/search/episodes | `podcasts-api.js`, `renderPodcasts*` | **Not integrated** | No podcast catalogue or episode feed is exposed by the wired providers; YouTube's saved-episode shelf was deliberately rejected as a fake catalogue. Nav has no dead Podcasts row |
| Listening parties | `listening-party.js`, `party-socket.js` | **Equivalent, separate** | Sonora Sessions (opt-in server, player-bar button, Members panel); not interoperable with Monochrome's party protocol |

## Playback and audio

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| One playback engine | `js/player.js` | **Native** | `crates/music/src/engine.rs`; Spotify is the documented librespot exception |
| Gapless playback | player preload/next | **Native** | Engine gapless path and Settings → Playback |
| Crossfade (with duration) | player crossfade | **Partial** | Spotify Soloist honours Spotify's own crossfade preferences; the shared engine has no local crossfade setting |
| Equalizer | `js/equalizer.js` | **Native** | `crates/state/src/equalizer.rs`, `screens/equalizer`; bands, profiles, per-device rules |
| AutoEQ / Equalizer APO import | `autoeq-importer.js`, `autoeq-data.js` | **Native** | `screens/equalizer/transfer.rs` imports APO text, reports skipped lines |
| Loudness normalisation / ReplayGain | `replay-gain.js` | **Partial** | Tag and provider gains are applied by the engine (Local, Subsonic, Jellyfin, Tidal); no pre-amp or album/track mode selector |
| Playback speed + preserve pitch | player speed | **Not integrated** | No speed control in the shared engine |
| Remove silence | player DSP | **Not integrated** | Not implemented |
| Mono / crossfeed / stereo width | DSP | **Not integrated** | Not implemented |
| Binaural / spatial DSP + HRTF presets | `binaural-dsp.js`, `hrtf-generator.js` | **Not integrated** | Not implemented; settings previews spatial audio only as a display concept |
| Dolby Atmos preference / OS rendering | Atmos settings | **Not integrated** | No Atmos path or claim |
| Streaming quality selection | quality settings | **Native** | Settings → Playback quality; provider-specific limits are respected |
| Quality badges on rows | quality badges setting | **Partial** | Quality is chosen and shown in settings; per-row "Hi-Res/Lossless" badges are not drawn |
| Visualizers (multiple styles) | `visualizer.js`, `visualizers/*` | **Partial** | One native spectrum visualizer (`crates/ui/src/visualizer.rs`, `music/src/spectrum`); no Butterchurn/LCD/particles/Unknown Pleasures styles |
| Waveform seekbar | `waveform.js` | **Not integrated** | Scrubber is a plain progress control |
| Output/device selection | audio settings | **Native** | `state::outputs`, Settings → Sound; device changes and fallback |
| Error recovery / next-source fallback | player error paths | **Native** | Playback error surfaces, provider fallback, queue advance, Downloads retry with backoff |
| SponsorBlock skipping | none | **Native (Soundy extra)** | `music/src/sponsorblock.rs`, Settings → Playback |
| Sleep timer | player sleep | **Not integrated** | No sleep timer |
| Exponential volume | volume setting | **Not integrated** | Linear volume only |
| Fullscreen cover click action | setting | **Not integrated** | Fullscreen cover opens the fullscreen shell; no configurable click action |
| Now-playing view modes | interface setting | **Native** | Player bar variants (`screens/settings/player_bar.rs`), fullscreen modes, right-panel modes |

## Lyrics, queue and right panel

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| Synced lyrics | `js/lyrics.js` | **Native** | LRCLIB, Musixmatch, NetEase, Kugou, Apple/Binimum TTML, local LRC |
| Word-by-word karaoke | TTML lyrics | **Native** | `chrome/aside.rs` karaoke sweep and word timing |
| Romanized lyrics | romaji setting | **Native** | Settings → Lyrics; CJK scripts enabled by default |
| Lyrics side panel | side panel | **Native** | Aside → Lyrics, pinned/follow behaviour |
| Queue panel and reorder | queue UI | **Native** | Aside → Queue; drag reorder, reset, clear, radio |
| Queue radio | player radio | **Native** | Queue header toggle when the provider offers stations |
| Members panel | parties | **Native** | Shown only inside a Sonora Session; Spotify Jam members are provider-driven |
| Track info / credits panel | metadata UI | **Not integrated** | No dedicated Info tab; metadata is shown in rows, player bar and detail pages |
| Related tracks/albums panel | recommendations | **Not integrated** | Related content is not a panel; discovery lives in Home/Discover |

## Downloads and offline

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| Explicit downloads | `downloads.js` | **Native** | `state::downloads`, Songs filter `filter-downloaded`, Downloads nav row |
| Offline playback | download service | **Native** | Encrypted vault/cache (`state::vault`), Downloads page, offline mode |
| Download queue and progress | download service | **Native** | Two at a time, progress, cancel, retry, ETA |
| Download quality choice | quality setting | **Partial** | Downloads follow the selected streaming quality; no separate download-quality ladder |
| File export with folder/filename templates | download writer | **Not integrated** | Soundy keeps downloads in its encrypted cache; it does not write a user-facing music folder |
| CUE / NFO / M3U / JSON sidecars | download formats | **Not integrated** | No file export pipeline |
| Lossless container choice / FFmpeg conversion | ffmpeg worker | **Not integrated** | No transcoding pipeline; provider streams are played as delivered |
| Download lyrics | lyrics setting | **Not integrated** | Lyrics are fetched/cached for display, not written beside exported files |
| Bulk download | bulk writer | **Partial** | Album/playlist download buttons queue every track; no separate bulk-selection screen |
| Cache size / clearing | cache settings | **Native** | Settings → Storage; measured cache size and clear action |

## Appearance and personalisation

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| Multiple themes | `themeStore.js` | **Native** | Settings → Appearance; built-in themes and theme links |
| Community theme gallery | community site | **Equivalent** | "Explore community themes" opens sonorahq.org; install links are handled natively |
| Custom theme colours | custom theme | **Native** | Theme editor and `ui::Theme` |
| Accent / dynamic colours | `vibrant-color.js` | **Native** | Emerald accent; artwork-derived ambient colour in fullscreen/aside |
| Album-cover background | appearance setting | **Partial** | Artwork palette drives the fullscreen/ambient background; the whole app does not paint the cover behind every page |
| Vanilla-tilt cover / 3D tilt | interface setting | **Not integrated** | No tilt effect |
| Font family | font setting | **Native** | Settings → Appearance typeface packs |
| Font size | font setting | **Native** | Settings → Appearance font size |
| Blur effects toggle | blur setting | **Native** | Settings → Appearance blur and window blur |
| Window opacity | opacity setting | **Native** | Settings → Appearance opacity |
| Compact albums/artists | interface setting | **Native** | Library grid/list modes and adaptive card sizing |
| Album release year | interface setting | **Native** | Album list year column and release metadata on album pages |
| Close modals on navigation / back | interface setting | **Native** | The modal layer closes on `Dismiss` and on navigation |
| EQ preset cycling | `equalizer.js` | **Not integrated** | Presets are chosen individually; there is no cycle shortcut |
| Cover-art size | interface setting | **Partial** | Card sizing is adaptive to width; no fixed cover-size slider |
| Sidebar order | sidebar settings | **Partial** | Rows can be hidden and items pinned; the fixed groups are not freely reorderable |
| Fullscreen cover click action | interface setting | **Not integrated** | No configurable action |
| Waveform seekbar | interface setting | **Not integrated** | Plain scrubber |
| Quality badges | interface setting | **Not integrated** | No per-row quality badge |
| Disable animations / reduced motion | CSS setting | **Native** | `Motion`/`Springs` respect the platform reduced-motion setting and in-app motion controls |

## Accounts, social and system

| Monochrome feature | Monochrome source | Soundy status | Soundy location / notes |
| --- | --- | --- | --- |
| Provider sign-in (Spotify, YouTube Music, Tidal, Deezer, Apple Music, Subsonic, Jellyfin) | `js/api.js`, `accounts/*` | **Native** | `crates/state/src/session.rs`, per-provider clients; cookie sign-in window only where required |
| Monochrome account / cloud profile | `accounts/pocketbase.js` | **Blocked** | No Monochrome account system or server; Soundy is local-first |
| Cloud sync / backup & restore | account settings | **Not integrated** | State stays in local SQLite; no cloud sync or settings backup file |
| Scrobbling: Last.fm, Libre.fm, ListenBrainz, Maloja | `lastfm.js`, `librefm.js`, `listenbrainz.js`, `maloja.js` | **Native** | `state::scrobble`, Settings → Scrobbling; thresholds and "love on like" |
| Discord Rich Presence | discord integration | **Native** | Settings → Discord, off by default |
| DEVICE / remote control | Spotify Connect, Jellyfin | **Native** | `state::devices`, Settings → Connect; Spotify Connect and Jellyfin remote |
| Auto-update | Tauri updater | **Not integrated** | Deliberately disabled for this fork; installer is manual |
| Donation reminders | settings | **Not integrated** | No donation prompts |
| Analytics / Sentry telemetry | `analytics.js`, `sentry.js` | **Not integrated** | No telemetry endpoint in Soundy |
| API instances / custom API credentials | `instances` settings | **Partial** | Provider servers (Subsonic/Navidrome, Jellyfin) can be configured; Monochrome's API mirrors do not exist here |
| Blocked content filter | `content-filter.js` | **Partial** | Explicit-content flag and hide-explicit filter; no blocklist editor |
| About / GitHub / Discord links | sidebar items | **Native** | Settings → About and title-bar account menu |
| Listening statistics | `listening-tracker.js` | **Not integrated** | History exists; aggregate play-count statistics/screens do not |
| Import/export settings | backup settings | **Not integrated** | Settings live in the state database; no portable settings file |

## What this release actually integrated

The parity work that landed in this build is not a reskin. These are the concrete additions:

1. **CSV/Exportify import, end to end.** `music::import` reads Spotify, Exportify, Soundiiz and
   TuneMyMusic exports, comma/semicolon/tab, BOM, artwork URLs, ISRC, track URIs/ids, added dates,
   release years, millisecond durations and headerless files. Rows keep their source order and
   duplicates.
2. **Metadata and artwork enrichment.** `music::enrich` uses the Deezer public API first and the
   iTunes Search API as a fallback, requires title + credited artist + duration (or album when a
   duration is unknown) and never substitutes a remix, live take or cover. A blocked or throttled
   provider is disabled for the rest of the import.
3. **Playlist collage covers.** `music::collage` builds a 2×2 (or 3/2/1-tile) JPEG from distinct
   track artwork, caches it in `%LOCALAPPDATA%\soundy\playlist-covers`, and the database records
   whether the cover is generated. A listener-chosen cover is never overwritten.
4. **Artwork everywhere the listener can see it.** Imported playlist headers, track rows, player bar,
   fullscreen player, queue, search, albums, artists, recents and recommendation cards share the
   same bounded artwork cache; a track without a length shows a dash, never `0:00`.
5. **Import dialog states.** Analysis, progress, metadata/artwork resolution, duplicates, ordering,
   summary, unavailable rows and a retry that only appears when a displayable row still lacks art.
   Everything runs off the UI thread.
6. **Navigation.** Discover (real genre browse), Favorites and Downloads as sidebar destinations,
   Settings at the foot of the navigation, and a sidebar that still lists imported playlists when
   no local folder is configured. A provider with no home shelves gets a stated empty state and a
   Discover action instead of a blank page.
7. **No dead podcast row.** The Podcasts destination is omitted rather than shipped empty; this
   document and `KNOWN-LIMITATIONS.md` say why.

## Deliberate non-goals

- Copying Monochrome's Tauri/React runtime, its PocketBase account service, or its community
  endpoints. None of those can be operated honestly by this build.
- Ripping, decrypting or redistributing protected streams, or claiming a format/quality the source
  did not deliver.
- Faking a podcast catalogue, a download-export pipeline, a spatial-audio engine or a crossfade
  that the shared engine does not have.
- Shipping disabled-looking buttons for features that are not implemented. A row appears only
  when it does something.

## Verification

- Unit and integration tests: `cargo test --workspace` (see `TEST-RESULTS.md`).
- Live importer check: `dist/import-test.csv`, five rows, Deezer artwork, 2×2 collage, one
  external row, statuses persisted.
- Manual UI pass: restored 936×648 window, imported playlist header, track covers, Favorites,
  Downloads, Discover, Settings placement, fullscreen player and right panel.
- Limits and provider blocks: `KNOWN-LIMITATIONS.md`; packaging and measured memory:
  `PERFORMANCE.md`.
