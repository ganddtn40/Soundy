# Architecture

Soundy is one native Windows application built from one Cargo workspace. There is no Electron
or Tauri shell, no embedded browser as the application UI, and no second player process.

## Crates

```
crates/
  sonora/     binary: window, actions, tray, assets, HTTP shim, provider wiring
  views/      screens, shells, dialogs and reusable app chrome
  state/      GPUI entities: session, library, playback, queue, import, settings
  music/      provider traits and models; one module per provider; playback engine
  storage/    state.sqlite and cache.sqlite paths, schema, migrations
  ui/         design system: theme, metrics, layout ladder, elements
  router/     destinations and navigation history
  input/      text input, global actions, keybindings
  i18n/       Fluent strings
  icons/      icon packs and asset source
  webview/    provider cookie sign-in window (not the app shell)
  widevine/   optional CDM host; no CDM is shipped
```

Dependency direction is strict: the binary wires concrete providers; `views` and `state` see
only the traits and models in the root of `music`; `music` never depends on GPUI; `ui` never
knows about playback or providers.

## One of each

- **One UI**: GPUI with the `ui` design system. Dialogs, including the importer, are normal GPUI
  modals inside the same window.
- **One playback engine**: `music::engine`, with progressive download in `music::stream` and the
  single loudness-normalisation path. librespot is the Spotify exception because it decodes
  inside its own player.
- **One library and queue**: local files and streaming providers share `music::Track`,
  `Playlist`, `Album` and the same detail screens. The queue is provider-independent.
- **One settings system**: `state::Settings` backed by the state database, rendered by one
  settings screen.
- **One data model**: provider modules convert their wire formats into the root `music` models.
  `views` never sees a provider-specific type.
- **One storage**: `state.sqlite` holds settings, the local library, imported playlists,
  favourites, downloads and playback history. `cache.sqlite` holds disposable caches.

## Playlist import path

```
dialog (views)  ->  state::Import  ->  music::import parsers
                        |                    |
                        |                    +-- Spotify link / CSV / M3U / manual
                        |                    +-- match_local
                        |
                        +-- active provider search (music trait)
                        +-- music::audius::Catalogue (Audius, optional Jamendo)
                        +-- music::enrich (Deezer, then iTunes: metadata and artwork)
                        +-- music::collage (2x2/3/2/1-tile playlist cover)
                        +-- local.save_import (music::local)
                                 |
                                 +-- storage::state.sqlite
```

- Enrichment runs after matching has settled: rows are grouped by album, covers already known are
  reused, and at most 160 album and 80 track lookups are made per import. A hit needs the title,
  the primary artist and the duration (or the album when there is no length), so a remix, live
  take or cover is refused. A provider that refuses or throttles the client is disabled for the
  rest of that import instead of being retried row by row.
- The playlist cover is built from distinct track pictures and written under
  `%LOCALAPPDATA%\soundy\playlist-covers`. The database records whether the cover was generated,
  so a later retry rebuilds a generated cover and never replaces one the listener chose.
- Artwork found by enrichment is cached under `%LOCALAPPDATA%\soundy\artwork`; the UI's image cache
  is separate, bounded at 16 MB / 256 pictures / 8 pending loads.

- Parsing and file reads run on the blocking pool.
- Local matching runs on the blocking pool with a bounded ISRC lookup set.
- Provider and catalogue searches run four at a time with bounded query caches.
- Only progress counters are shared with the UI; the final report is one entity update.
- Cancellation is an atomic flag checked between requests and before the transaction.
- Imported playlists are stored in `imported_playlists` and `imported_entries`, with
  `external_url` added by an in-place migration for databases created by an earlier build.

The status of every row is persisted, so a reopened playlist shows the same labels without a
network request. Playback of a **Full playback** row uses the stored track id; **Full playback
(local)** rows resolve to the listener's scanned file.

## Threads and resources

- Four tokio worker threads for network and JSON work.
- Eight blocking threads for SQLite, artwork, tag reads and file parsing, with a short linger.
- One HTTP connection pool for the importer's catalogue lookups.
- No polling loop while idle. The importer's progress timer exists only while an import runs.
- Imported playlists and the import report are bounded by the playlist size; search caches are
  bounded at 2048 queries per import.

## Privacy boundary

There is no Soundy-operated server. Provider credentials stay in local storage; play history
and cached tracks stay on disk. The importer sends a search query to the active provider and to
Audius only for rows that still need matching, and to Jamendo only when the listener configured
a client id. Artwork enrichment sends only the row's title, artist and album to Deezer and, when
Deezer has no confident hit, to iTunes. A local file or pasted list is never uploaded.
