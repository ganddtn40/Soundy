# Feature comparison

This matrix compares the shipped Soundy 1.0.0 build with the upstream Sonora project it is
built from, and checks each capability the unified-desktop brief asked for. The last column is
the requirement, not a claim about another product. The feature-by-feature audit of Monochrome
Desktop, including every item deliberately not integrated, is in `MONOCHROME-PARITY.md`.

Legend: **Yes** — present and exercised in this build. **Same** — inherited unchanged from
Sonora. **Partial** — present with a stated limit. **No** — not present.

| Capability | Soundy 1.0.0 | Upstream Sonora | Unified target | Evidence |
| --- | --- | --- | --- | --- |
| Native Windows application | Yes | Yes | Yes | Rust + GPUI, `soundy.exe`, no Electron/Tauri shell |
| One process and one UI | Yes | Yes | Yes | One GPUI window; the WebView2 helper is used only for provider cookie sign-in, never as the app shell |
| One playback engine | Yes | Yes | Yes | `music::engine`; librespot only for Spotify |
| One library | Yes | Yes | Yes | Local and streaming shelves share the `music` models and the same detail/list screens |
| One queue | Yes | Yes | Yes | Playback queue is provider-independent |
| One settings system | Yes | Yes | Yes | `state::Settings`, one settings screen |
| One data model | Yes | Yes | Yes | `music` root models; providers only implement traits |
| Local file playback | Yes | Yes | Yes | Local provider, tags, artwork, spectrum |
| Streaming providers | Yes | Yes | Yes | Spotify, YouTube Music, Tidal, Deezer, Apple Music, Subsonic/Navidrome, Jellyfin |
| Playlists | Yes | Yes | Yes | Create, rename, delete, reorder, import, export |
| Smart playlists | Yes | Yes | Yes | Rule editor with boolean and order rules |
| Import from Spotify link | Partial | No | Yes | With an existing authorized Spotify session; without one, a clear `TracklistUnavailable` result and file/paste fallbacks |
| Import M3U/M3U8 | Yes | Partial | Yes | Local paths, `file://`, relative paths, HTTP entries as external links |
| Import CSV | Yes | No | Yes | Spotify/Exportify/Soundiiz/TuneMyMusic exports, comma/semicolon/tab, BOM, artwork URLs, ISRC, URIs, dates, ms durations and headerless files |
| Artwork enrichment | Yes | No | Yes | Deezer public API first, iTunes fallback; strict title + artist + duration/album matching; blocked providers disabled for the import |
| Generated playlist covers | Yes | No | Yes | 2×2/3/2/1-tile collage from distinct track art, cached, marked generated so a listener's own cover is kept |
| Artwork on every surface | Yes | Partial | Yes | Playlist headers, rows, player bar, fullscreen, queue, search, albums, artists, recents and picks share one bounded cache |
| Import retry | Yes | No | Yes | Re-runs enrichment for saved rows; hidden when only unplayable rows lack art; never overwrites a chosen cover |
| Unknown duration display | Yes | Partial | Yes | A dash, never a fabricated `0:00` |
| Discover screen | Yes | Partial | Yes | Genre browse over the shared `Genres` entity |
| Favorites / Downloads destinations | Yes | Partial | Yes | Sidebar rows open Songs narrowed by `filter-favorites` / `filter-downloaded` with named empty states |
| Settings in the sidebar | Yes | Yes | Yes | Last row of the navigation, hideable like the others |
| Podcasts | No | No | Documented unsupported | No wired provider exposes a podcast catalogue; no dead nav row is shipped |
| Import pasted list | Yes | No | Yes | `Artist - Title` or pipe-separated columns |
| Match imported rows to local files | Yes | No | Yes | Exact path, title/artist/album/length, ISRC tie-break |
| Match imported rows to the active provider | Yes | No | Yes | Exact recording only; no remix/live substitution |
| Public catalogue links without an account | Yes | No | Yes | Audius; Jamendo when `SOUNDY_JAMENDO_CLIENT_ID` is set |
| Per-row playback status | Yes | No | Yes | Full playback, Full playback (local), Preview only, External playback, Unavailable |
| Keep order and duplicates | Yes | No | Yes | Positions stored; duplicates counted, not removed |
| Keep unplayable rows | Yes | No | Yes | Read-only imported playlist retains every row |
| Lyrics | Yes | Yes | Yes | LRCLIB, Musixmatch, NetEase, Kugou, local LRC, romanization |
| Equalizer | Yes | Yes | Yes | Bands, profiles, per-device rules |
| Loudness normalisation | Partial | Yes | Yes | Tag and provider gains are applied by the shared engine; there is no pre-amp or album/track mode selector |
| Downloads / offline | Yes | Yes | Yes | Encrypted played-track cache and explicit downloads |
| Scrobbling | Yes | Yes | Yes | Last.fm, Libre.fm, ListenBrainz, Maloja |
| Discord presence | Yes | Yes | Yes | Off by default |
| Tray and media keys | Yes | Yes | Yes | Native tray, taskbar thumb bar, media controls |
| Listening sessions | Yes | Yes | Yes | Upstream session server, opt-in |
| Self-update | No | Yes | No | Disabled in Soundy |
| Install reporting | No | Optional | No | Disabled; no telemetry endpoint |
| Windows installer | Yes | Yes | Yes | Inno Setup, `dist\Soundy-1.0.0-Setup.exe` |
| Portable build | Yes | No | If practical | `dist\Soundy-1.0.0-Portable.zip` |
| Code signing | No | Upstream policy | No certificate | Unsigned local build; SmartScreen may warn |
| Idle memory below 100 MB | No | Not measured here | Yes | 110.1 MB working set, 98.7 MB private commit; resident target missed, commit under it. See `PERFORMANCE.md` |
| Idle CPU near zero | Yes | Not measured here | Yes | 0.344 s of CPU per 60 s idle (≈0.6% of one core) |
| No background polling | Yes | Yes | Yes | No polling loops; import progress timer only while active |

## What "unified" means in this build

- **One executable.** The player, importer, settings, library and queue all live in `soundy.exe`.
- **One visual system.** Import screens use the same UI primitives, theme, spacing ladder,
  Fluent strings and accessibility roles as the rest of the app.
- **One storage.** Imported playlists live in the same SQLite state database and appear in the
  same library list as local and streaming playlists.
- **One matching pipeline.** Local, provider and catalogue answers are resolved by the same
  `music::import` rules for every source.
- **No wrapper.** There is no Electron, Tauri, embedded browser shell or second player process.
  The only WebView2 use is the existing cookie sign-in window some providers require.

## Deliberate non-goals

- Ripping, decrypting or redistributing protected streams.
- Scraping Spotify's web player or bypassing its authentication.
- Claiming a track is playable when only an external page is known.
- Replacing the listener's library with a cloud account.
