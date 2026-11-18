# Changelog

All notable changes to Soundy are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-11-18

The first Soundy release: a native Windows music player built on the Sonora codebase.

### Added

- A complete desktop player for local files and the streaming providers Sonora supports,
  written in Rust with GPUI and rendered without Electron, a WebView shell or a JavaScript UI
  framework.
- The Soundy identity: window title, tray, taskbar, notifications, application id
  `app.soundy.player`, the `soundy:` link scheme, `soundy.exe`, a waveform icon and a Windows
  installer.
- An emerald Liquid Glass theme over graphite surfaces, with frosted controls and a fullscreen
  player that presents the cover as a vinyl record over a blurred artwork field.
- One integrated library: Songs, Albums, Artists, Playlists, Downloads and Favorites share the
  streaming and local collections, with a single artwork cache behind every screen.
- Artist, album, playlist and genre pages, search with grouped results and recent searches,
  playback history and a persistent queue.
- A fullscreen now-playing view, lyrics with word timing, a queue panel, an equalizer with
  profiles and per-device rules, and a native spectrum visualizer.
- Playlist import from a Spotify playlist link, M3U/M3U8 or CSV files, or a pasted track list.
  No Spotify account, Premium, OAuth or API key is required; a link needs an existing Spotify
  session to read its tracklist, and the dialog says so instead of scraping.
- Imported playlists keep their order and duplicate counts, get provider-matched metadata and
  artwork, and fall back to a generated 2x2 cover when no artwork exists.
- Optional Last.fm, Libre.fm, ListenBrainz and Maloja scrobbling, Discord rich presence, and
  Sonora Session listening rooms.
- Portable ZIP and Inno Setup installer, both built from the same release binary.

### Changed

- The left sidebar keeps the navigation focused: Home, Search, Recents, Songs, Albums, Artists,
  Playlists, Downloads and Favorites. Settings opens from the title-bar gear.
- The page width, backdrop blur and motion can be adjusted or turned off in Settings,
  Appearance.
- Update checking and the updater are removed, so a Soundy build is never replaced by a Sonora
  release.

### Fixed

- Playback, queue and artwork stay consistent when the active provider changes or a track is
  unavailable.
- The installer removes stale portable staging before packaging, so a rebuild never ships old
  files.

### Known limits

- Windows 11 x64 is the only packaged target. Linux and macOS sources remain in the tree but
  are not released.
- The executable is unsigned, so Windows SmartScreen may warn on first launch.
- The Spotify playlist link path needs an existing Spotify session; without one, use a file or
  a pasted list.
- Idle resident memory measures about 110 MB, roughly 10 MB above the 100 MB target. The
  measured figures and method are in [PERFORMANCE.md](./PERFORMANCE.md).

[Unreleased]: https://github.com/ganddtn40/soundy/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/ganddtn40/soundy/releases/tag/v1.0.0
