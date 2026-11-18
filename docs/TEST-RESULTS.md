# Test results

Everything below was run on this machine against this source tree. A test that was not run is
listed as not run, not as passed.

## Automated

Command:

```powershell
cargo test --workspace
```

Result: **673 passed, 0 failed, 20 ignored** across 13 test binaries, plus doc-tests (0 tests).

| Crate / binary | Tests | Result |
| --- | ---: | --- |
| `cli` | 31 | pass |
| `i18n` | 8 | pass |
| `icons` | 1 | pass |
| `input` | 5 | pass |
| `music` | 321 | pass (20 ignored) |
| `sonora` (binary) | 15 | pass |
| `state` | 110 | pass |
| `storage` | 2 | pass |
| `ui` | 74 | pass |
| `ui` accessibility tree | 32 | pass |
| `views` | 57 | pass |
| `webview` | 4 | pass |
| `widevine` | 13 | pass |

The 20 ignored `music` tests are the live-network suite (`crates/music/src/live_tests`), which
needs real provider credentials and is skipped by default.

`cargo fmt --all -- --check` passes. `cargo build --release -p sonora` completes with no errors.

### What the importer tests cover

- Spotify link forms: web URL with and without query, `/intl-*/`, `/embed/`, trailing slash,
  bare host, `spotify:playlist:` and the legacy `spotify:user:…:playlist:` form.
- Refusal of track, album and artist links, other services and empty input.
- M3U/M3U8: `#EXTINF` duration and credit, relative paths, `file://` with percent-encoding and
  Windows drive letters, absolute paths, HTTP(S) entries kept as external rows, Latin-1 files,
  skipped rows.
- CSV: header in any order or case, no header, quoted commas, line breaks inside quotes,
  `m:ss` and `h:mm:ss` durations, missing title, skipped rows.
- Pasted list: `Artist - Title`, pipe-separated columns, comments, title-only rows.
- Matching: exact title and primary artist, album and length tolerance, ISRC tie-break,
  refusal of remix/live/different-artist answers, local path equality.
- Report counts: available, personal, preview, external, unavailable, duplicates, skipped, and
  the unmatched list in source order.
- Storage: `external_url` migration on an old database and on a fresh one; re-import maps to the
  same playlist by `(source, source_id)` and replaces rows inside one transaction.
- Audius: response parsing and strict recording equality, plus two live HTTP checks that were
  run separately (see below).

## Manual verification, release build

The release binary `target\release\soundy.exe` (90,669,056 bytes, built 18 November 2026) was
launched on Windows 11 and driven with real mouse and keyboard input. The dialog is reached
through **+ → Import playlist…** on the Playlists screen. Provider matching ran against the
signed-in YouTube Music account. Every row count below was also checked in
`%LOCALAPPDATA%\soundy\state.sqlite`, not only in the dialog.

| Check | Result |
| --- | --- |
| App launches to the library with the restored session | pass |
| Import dialog opens with Spotify link / M3U or CSV file / Pasted list tabs | pass |
| Invalid link `https://example.com/not-spotify` | **Import failed**: “That is not a Spotify playlist link. Paste a link like https://open.spotify.com/playlist/… or a spotify:playlist: URI.” |
| Valid Spotify playlist URL (`37i9dQZF1DXcBWIGoYBM5M`) with no Spotify session | **Import failed**: “Spotify does not publish this playlist's tracks without an authorized Spotify session. Soundy does not scrape the page or bypass that.” The dialog tells the listener to export M3U/CSV or paste the rows; no playlist is created |
| Pasted list of four rows, including a duplicate and a nonexistent song | 4 tracks kept, 2 Full playback, 2 Unavailable, 1 duplicate kept; result dialog and playlist both show the rows in source order |
| CSV through the real Windows file picker (`Soundy Import Test.csv`) | 6 tracks kept, 2 playable, 4 Unavailable, 1 duplicate kept; durations `5:02` and `3:33` preserved; `Hardwired (Live)` correctly refused as a different recording |
| M3U through the real file picker, two entries (one local path, one `https://` URL) | local row matched the scanned file and became **Full playback (local)**; HTTP row became **External playback**; order preserved |
| Re-importing the same M3U after changing one title | replaced the same playlist by `(source, source_id)`, not a second copy; the open detail screen refreshed in place to `Stream Test v2` |
| Large CSV, 1,000 rows (one distinct track repeated) | 1,000 tracks kept, 1,000 playable, 999 duplicates kept, 0 unmatched; `imported_entries` holds 1,000 rows at positions 0–999; the result dialog was on screen within 2 seconds of pressing Import |
| Offline: app started with `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` pointed at a dead port, then a two-row list pasted | import completed locally: 2 tracks kept, 0 playable, 2 Unavailable. No fake match, no hang |
| External catalogue badge shows a link and opens it | pass |
| Idle after import | 45 one-second samples, median CPU 0.00%, no repeated requests in the log |

### Regressions found during manual testing

**Provider search aborted the process.** The first release build of the importer aborted when a
provider search ran. The import task was calling provider and catalogue HTTP work on GPUI's
executor, which has no Tokio reactor; `ytmusic-rs` uses `tokio::time::timeout`, which panicked on
a background thread and aborted the app. Searches now run through `Io::spawn` on the Tokio runtime
that owns the provider connections. The abort is not present in the shipped build, and the manual
checks above were repeated against the rebuilt binary.

**An open playlist kept stale rows after a re-import.** The database was updated, but a playlist
detail page that was already open kept its cached copy and still showed the old statuses (an M3U
row that was now `personal` still read “Unavailable”). The import now invalidates the local
catalogue playlist cache before emitting `LibraryEvent::PlaylistChanged`, and the detail page
reloads on that event. Reproduced with the real app, fixed, rebuilt, and re-verified: the open M3U
playlist immediately showed `test-tone — Full playback (local)` and `Stream Test v2 — External
playback`.

### Automation limits

GPUI exposes no Windows UI Automation tree in this build, so the dialog was driven by screen
coordinates and real input. Some synthetic tab clicks did not register at the expected pixel; the
tests above were repeated with keyboard focus or a different position and confirmed in both the
UI and the database. This is a limitation of the harness, not a claim that the tab is broken —
the same tab switch worked when driven by focus and by a different click.

## Release artifacts

Built on 18 November 2026 from this tree with `scripts\windows\build-soundy-installer.ps1`:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `target\release\soundy.exe` | 90,669,056 bytes | `D854C17723D89F0C2D6B81EE749DF7D2003DC3D7CAEE465772178E17832A6062` |
| `dist\Soundy-1.0.0-Setup.exe` | 23,960,099 bytes | `6D220B890FBA59721CD2E4364C0EB16F1634FB6E107064301736EDB402BE206E` |
| `dist\Soundy-1.0.0-Portable.zip` | 32,921,721 bytes | `139480B08CAA8EBDEEBDE3E00975883153F39ACE63D5B0639C446817E736DA7B` |

Both the installed executable and `dist\Soundy-1.0.0-Portable\soundy.exe` hash to the release
binary above. The setup file reports version 1.0.0 and product Soundy.

The installer was also executed, not just compiled: silent install to a temporary directory
returned 0, produced `soundy.exe` with the matching hash, plus `LICENSE`, `THIRD-PARTY.md` and the
uninstaller. The uninstaller returned 0 and removed the install directory and Start Menu shortcut.
The portable ZIP was unpacked and its `soundy.exe` launched and restored the library.

## Performance

Final-build figures are in `PERFORMANCE.md`: 111.7 MB median idle RSS, 100.0 MB private working
set, 0.00% median idle CPU, 193 ms median startup, and 128.3 MB median RSS while playing local
files. The measurement profile had YouTube Music signed in and five imported playlists, so it is
not a fresh-profile minimum.

## Live checks

- Audius search without an account:
  `https://api.audius.co/v1/tracks/search?query=Hardwired%20Rameses%20B&limit=3` returned
  `Rameses B - Hardwired`, 302 s, streamable, permalink
  `/RamesesB/rameses-b-hardwired-431301`. This is the same query the importer builds for that row.
- Spotify Web API: not exercised, because no Spotify account is signed in on this machine. The
  authorized path is covered by unit tests against a local HTTP server only.
- Jamendo: not exercised, because `SOUNDY_JAMENDO_CLIENT_ID` is not configured.
- M3U and CSV imports: exercised with real files written for the test
  (`Soundy Import Test.m3u`, `Soundy Import Test.csv`) and the pasted list
  (`Rameses B - Hardwired`, a duplicate, an unknown song, a title-only row).

## Not tested

- A real private Spotify playlist and its `Forbidden` answer: no Spotify account.
- A real Spotify playlist with the authorized path: no Spotify account.
- Jamendo matching with a real client id.
- A large list of many *distinct* tracks: the 1,000-row test repeated one recording, so it
  exercised row count, duplicate preservation and UI performance but only one distinct provider
  query. A list with hundreds of distinct queries was not timed.
- Pixel and accessibility-tree automation of the dialog: GPUI exposes no Windows UI Automation
  tree in this build, and the project has no GPUI test harness. The dialog was driven by real
  input instead.
- macOS and Linux builds: only Windows x64 is packaged here.
- Long-duration soak tests beyond the idle measurement in `PERFORMANCE.md`.
