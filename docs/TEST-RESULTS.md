# Test results

Everything below was run on this machine against this source tree at the date noted for each
section. A test that was not run is listed as not run, not as passed.

## Automated

Command:

```powershell
cargo test --workspace
```

Result: **706 passed, 0 failed, 21 ignored** across 13 test binaries with tests, plus doc-tests (0 tests). Run on 19 November 2026 against the 1.0.1 tree.

| Crate / binary | Tests | Result |
| --- | ---: | --- |
| `cli` | 31 | pass |
| `i18n` | 8 | pass |
| `icons` | 1 | pass |
| `input` | 5 | pass |
| `music` | 349 | pass (21 ignored) |
| `soundy` (binary) | 15 | pass |
| `state` | 112 | pass |
| `storage` | 4 | pass |
| `ui` | 74 | pass |
| `ui` accessibility tree | 32 | pass |
| `views` | 58 | pass |
| `webview` | 4 | pass |
| `widevine` | 13 | pass |

The 21 ignored `music` tests are the live-network suite (`crates/music/src/live_tests`) plus the live
iTunes artwork lookup, which need real provider credentials or the public catalogue and are skipped
by default. The live iTunes lookup was run separately and passed (see below).

The new 1.0.1 tests include seven for the anonymous public-page reader: HTML entity and NBSP
cleanup, track and artist extraction, preview and playable flags, the 100-row limit notice, a
missing entity mapping to not found, a changed page shape mapping to `PublicPageChanged`, and retry
and backoff behaviour.

`cargo fmt --all -- --check` passes. `cargo build --release --locked -p soundy` completes with no
errors.

### What the importer tests cover

- Spotify link forms: web URL with and without query, `/intl-*/`, `/embed/`, trailing slash,
  bare host, a bare 22-character ID, `spotify:playlist:` and the legacy
  `spotify:user:…:playlist:` form.
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
- CSV artwork and metadata: Spotify `Album Image URL`, Soundiiz and TuneMyMusic headers, `ISRC`,
  `Track URI`, `Track ID`, `Added At`, release years, millisecond durations, semicolon and tab
  delimiters, byte-order marks and headerless files.
- Enrichment: exact title and primary artist, duration within tolerance, album fallback when the
  list gave no length, refusal of remix/live/other-artist hits, album grouping, request caps, and
  a provider that refuses or throttles being disabled for the rest of the import.
- Collage: 4/3/2/1-tile compositions, distinct pictures only, JPEG output, decode bounds and the
  emerald fallback card.
- Cover ownership: a generated collage is rebuilt by a retry, while a listener-chosen picture is
  kept; `cover_generated` survives the database round trip.
- Artwork retry: `missing_art` counts only rows that can display a picture, and the retry button
  is hidden when only external or unavailable rows lack one.
- Navigation: the two sidebar groups cover every row without overlap, so a row can never be
  drawn twice.

## Manual verification, 1.0.0 build (historical)

This section records the 1.0.0 manual pass and is kept as the regression baseline for file and
pasted-list imports. The 1.0.1 Spotify import is verified separately below. The release binary
`target\release\soundy.exe` (91,252,736 bytes, built 19 November 2026 before the metadata sign-in
and review changes) was launched on Windows 11 and driven with real mouse and keyboard input. The
dialog is reached through **+ → Import playlist…** on the Playlists screen.
Provider matching ran against the signed-in YouTube Music account. Every row count below was also
checked in `%LOCALAPPDATA%\soundy\state.sqlite`, not only in the dialog. The 1.0.1 re-test
follows this section; where the two differ, the re-test describes what ships.

| Check | Result |
| --- | --- |
| App launches to the library with the restored session | pass |
| Import dialog opens with Spotify link / M3U or CSV file / Pasted list tabs | pass |
| Invalid link `https://example.com/not-spotify` | **Import failed**: “That is not a Spotify playlist link. Paste a link like https://open.spotify.com/playlist/… or a spotify:playlist: URI.” |
| Valid Spotify playlist URL (`37i9dQZF1DXcBWIGoYBM5M`) with no Spotify session | 1.0.0 baseline: **Import failed** because no Spotify session could read the playlist. The 1.0.1 anonymous public-page path replaces this behaviour; see the 1.0.1 section. |
| Pasted list of four rows, including a duplicate and a nonexistent song | 4 tracks kept, 2 Full playback, 2 Unavailable, 1 duplicate kept; result dialog and playlist both show the rows in source order |
| CSV through the real Windows file picker (`Soundy Import Test.csv`) | 6 tracks kept, 2 playable, 4 Unavailable, 1 duplicate kept; durations `5:02` and `3:33` preserved; `Hardwired (Live)` correctly refused as a different recording |
| M3U through the real file picker, two entries (one local path, one `https://` URL) | local row matched the scanned file and became **Full playback (local)**; HTTP row became **External playback**; order preserved |
| Re-importing the same M3U after changing one title | replaced the same playlist by `(source, source_id)`, not a second copy; the open detail screen refreshed in place to `Stream Test v2` |
| Large CSV, 1,000 rows (one distinct track repeated) | 1,000 tracks kept, 1,000 playable, 999 duplicates kept, 0 unmatched; `imported_entries` holds 1,000 rows at positions 0–999; the result dialog was on screen within 2 seconds of pressing Import |
| Offline: app started with `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` pointed at a dead port, then a two-row list pasted | import completed locally: 2 tracks kept, 0 playable, 2 Unavailable. No fake match, no hang |
| External catalogue badge shows a link and opens it | pass |
| Idle after import | 45 one-second samples, median CPU 0.00%, no repeated requests in the log |

### Unified build screens (release binary)

The unified 1.0.0 build was launched at 936×648 and driven with real clicks and keystrokes. The
same rows were checked against `%LOCALAPPDATA%\soundy\state.sqlite` and the cover files on disk.

| Screen / state | Result |
| --- | --- |
| Home | Quick picks plus the stated empty state when the provider sends no shelves: “Nothing on your provider's home page” and a working “Browse genres” action to Discover |
| Playlists | `import-test` listed with its 2×2 collage and “5 tracks” |
| Playlist detail | 2×2 collage header, “5 songs • 12m 44s”, four track covers, `3:20`, `3:23`, `2:47`, `3:14`, and a dash for the row the export never measured |
| Favorites | hero “Favorites”, “0 songs”, “No favorites yet” (not the folder prompt) |
| Downloads | hero “Downloads”, “0 songs”, “Nothing downloaded yet” (not the folder prompt) |
| Discover | provider genre grid with cover plates (Chill, Commute, Energize, Feel good, …) |
| Settings | title-bar gear, opening General/Appearance/Lyrics/Playback/Integrations/Keybinds |
| Search | best match plus grouped results, each with a cover and a real duration |
| Recents | history rows with covers, relative played times and durations |
| Right panel | Lyrics for the playing track; Queue with History, Now playing and Up next, including the external row’s placeholder and no `0:00` |
| Fullscreen player | opens from the player-bar button and `F`; large artwork, warm artwork-derived background, lyrics, seek and full transport; `Escape` leaves |

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

## Manual verification, 1.0.1 build

Run on 19 November 2026 against the 1.0.1 release binary (`target\release\soundy.exe`,
90,958,848 bytes, SHA-256 `32E7ED68…DAC38A`) at 936×648 on Windows 11, with real mouse and
keyboard input. The database was inspected after every step. Before testing, the profile was
backed up; it was restored afterwards and matched its original `state.sqlite` and `settings.json`
hashes byte-for-byte.

| Check | Result |
| --- | --- |
| Import dialog copy | opens from **+ → Import playlist…** with “Paste a public Spotify playlist link, URI or 22-character playlist ID… no account, sign-in or Premium…” and Import disabled while the field is empty |
| Non-Spotify URL `https://example.com/not-spotify` | stays quiet while typing, Import stays disabled, and no request is made |
| Spotify track link | inline “That is not a Spotify playlist link…” and Import disabled |
| Nonexistent playlist ID (`0000000000000000000000`) | **Import failed — Spotify could not find that playlist. Check the link and that the playlist still exists.**; the Choose a file / Paste a list fallbacks are offered and no playlist or row is written |
| Supplied playlist pasted with its `?si=` parameter | fetched anonymously; 91 tracks read from the public page; 37 ambiguous rows were offered for review and the preselected first matches were accepted; saved as `idgaf world!` |
| Result screen | 91 tracks kept, 83 playable, 8 from local files, 0 preview, 8 external, 0 unavailable, 90 with artwork; the original Spotify cover is shown |
| Database | playlist `local-playlist:1795083989485-0`, source `spotify`, source_id `3nBdBqQO6hVxIR6TMwF52H`, owner `zyvx`, total 91, duplicates 0, `cover_generated` 0, cover URL on Spotify's CDN; positions 0–90; statuses 75 available + 8 personal + 8 external; order preserved |
| Duplicate rows | two `Coming Down` rows kept at positions 2 and 49 with distinct Spotify ids and durations (295,453 ms and 295,435 ms) |
| Artwork | 90 of 91 rows carry a cover; only position 24 lacks one, and it is an external row with no catalogue answer |
| Playlist detail | `idgaf world!`, `zyvx • 91 songs • 6h 12m`, original cover, album names and covers on the rows, **Full playback** and **Full playback (local)** badges |
| Result footer **Open playlist** | closes the dialog and opens the saved playlist |
| Result footer **Import another** | returns to an empty Link field with Import disabled |
| Re-import by bare ID `3nBdBqQO6hVxIR6TMwF52H` | replaced `local-playlist:1795083989485-0` in place; still 3 playlists and 367 rows, not a second copy |
| Cancel during matching | **Import cancelled — The import was cancelled before anything was saved.**; the database kept its previous 3 playlists and 367 rows |
| Limit notice | not shown; the playlist has 91 tracks, below the public page's 100-row cap |

A second pass in guest mode, after the saved YouTube Music cookies expired mid-session, read the
same 91 tracks and reached the same result screen. It produced 85 playable, 8 local and 6
external rows because no provider search was available. The import therefore does not depend on a
provider session; only the number of playable matches does.

### Harness note

The first attempt to click the result footer landed at y≈511, on the explanatory text above the
buttons, and did nothing; the buttons sit at y≈555 in the 936×648 window. Clicking the correct row
exercised both actions. This was a coordinate error in the harness, not a hit-testing defect.

## Release artifacts

Built on 19 November 2026 from this tree with `scripts\windows\build-soundy-installer.ps1`:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `target\release\soundy.exe` | 90,958,848 bytes | `32E7ED68E39DEFACE41FED52CDFF09DA8A059B3F7D6FF832BB7BD31BD6DAC38A` |

The installed executable and `dist\Soundy-1.0.1-Portable\soundy.exe` hash to the release binary
above. The setup and ZIP hashes are published on the release page instead of here: every archive
bundles this document, so a hash written into it would change the archive it describes. The setup
file reports version 1.0.1 and product Soundy.

The installer was executed, not just compiled: a silent install to a temporary directory
returned 0, produced `soundy.exe` with the matching hash, plus `LICENSE`, `THIRD-PARTY.md` and the
uninstaller. The uninstaller returned 0 and removed the install directory and Start Menu shortcut.
The portable ZIP was unpacked and its `soundy.exe` launched with the restored profile; it carries
`README.md` and `PERFORMANCE.md` beside the licence and notices.

## Performance

The 1.0.1 release and the published 1.0.0 baseline were measured on the same machine at 936×648,
with 45-second settle and 60-second samples, each profile measured twice in opposite order. Full
tables are in `PERFORMANCE.md`.

- **1.0.1 fresh profile:** 104.0–104.3 MB working set / 87.9–89.9 MB private commit, 0.30–0.41 s
  of CPU per idle minute, 201 ms median startup (198, 201, 203, 191, 202).
- **1.0.1 populated profile** (signed-in provider, two imported playlists, 276 rows):
  119.4–120.8 MB / 100.0–101.5 MB, 0.34–0.53 s per minute.
- **1.0.0 on the same profiles:** 103.9–107.9 MB / 87.9–99.1 MB fresh and 120.0–120.9 MB /
  100.5–102.1 MB populated, 0.33–0.56 s per minute, 192 ms median startup.

The spread within one version is larger than the gap between versions, so the comparison shows no
measurable regression. The 100 MB resident target is missed by about 4 MB on a fresh profile;
private commit meets it there and is at the boundary on a populated profile. Streaming was not
re-measured because the playback path did not change; the earlier 1.0.0 figure is 124.5 MB /
107.6 MB. Both 60-second windows were flat, showing no leak.

## Live checks

- Anonymous Spotify public page: `https://open.spotify.com/embed/playlist/3nBdBqQO6hVxIR6TMwF52H`
  returned HTTP 200 to a `Soundy/1.0.1` user agent with 91 rows in `__NEXT_DATA__`, owner `zyvx`,
  the playlist title and cover URL. A nonexistent ID also returned HTTP 200, with
  `pageProps.status: 404`, which the reader maps to `NotFound`. A page with 100 rows is flagged
  `limited`; this playlist is below the cap.
- Audius search without an account:
  `https://api.audius.co/v1/tracks/search?query=Hardwired%20Rameses%20B&limit=3` returned
  `Rameses B - Hardwired`, 302 s, streamable, permalink
  `/RamesesB/rameses-b-hardwired-431301`. This is the same query the importer builds for that row.
- Spotify Web API: the metadata-only sign-in was launched from a real playlist link in the 1.0.0
  pass. The browser opened Spotify's authorize page with the expected client id, PKCE challenge,
  `state`, both playlist-read scopes and the `http://127.0.0.1:8989/login` redirect, and Spotify
  rendered its consent screen. Consent was cancelled deliberately, so no authorization code was
  exchanged. The HTTP reader, callback parsing and refresh logic are covered by unit tests against
  a local HTTP server.
- Jamendo: not exercised, because `SOUNDY_JAMENDO_CLIENT_ID` is not configured.
- M3U and CSV imports: exercised in the 1.0.0 pass with real files written for the test
  (`Soundy Import Test.m3u`, `Soundy Import Test.csv`) and a pasted list.
- The 1.0.1 dialog, review screen, result actions and playlist page were driven with real mouse
  and keyboard input at 936×648. GPUI exposes no UIA tree in this build, so screen coordinates
  were used (see the harness note above).

## Playlist importer re-test, 1.0.0 build (historical)

Run on 19 November 2026 against the rebuilt release binary and the packaged installer and portable
ZIP:

- **Invalid link.** Pasting `https://open.spotify.com/playlist/this-is-not-a-playlist` showed the
  invalid-link message and wrote no playlist.
- **Sign-in launch and cancel.** A real playlist link started the metadata sign-in, opened the
  browser on Spotify's authorize page (see above), and kept Soundy listening on port 8989. Cancel
  closed the listener within two seconds, showed "Import cancelled", and left no `metadata.json`.
- **CSV import with review.** A five-row Spotify export imported in source order. The provider
  returned six playable matches for one row, the review screen listed them with album and length,
  a non-first candidate was selected, and the saved playlist contained five playable/personal
  rows. The reviewed row's saved provider id played; loopback capture of a provider row was
  non-silent (peak 0.985, RMS 0.228 over eight seconds).
- **Cancel during matching.** A 100-row pasted list was cancelled at "Matching 0 of 100 tracks";
  the database still held its previous two playlists and 276 rows. The pre-test database backup
  was then restored and matched its original SHA-256 exactly.
- **Packaging.** The rebuilt installer silent-installed to a temporary directory and its app
  launched; the uninstaller removed it. The rebuilt portable ZIP was unpacked, launched, and
  carried the updated `docs/`.

## Not tested

- A private, collaborative, empty or region-restricted playlist: the anonymous public page cannot
  read one without a signed-in Spotify session, so only the nonexistent/`NotFound` path was
  exercised live. The failure mapping is unit-tested.
- A playlist longer than 100 tracks: the public page caps its list at 100 and the `limited` notice
  is unit-tested, but no live playlist over the cap was imported.
- A granted Spotify consent and the resulting Web API playlist read: the consent screen was
  reached, then cancelled, so no authorization code was exchanged with Spotify itself.
- A playlist owned by another account and its `Forbidden` answer through the Web API.
- Jamendo matching with a real client id.
- A large list of many *distinct* tracks: the 1,000-row test repeated one recording, so it
  exercised row count, duplicate preservation and UI performance but only one distinct provider
  query. The 91-track Spotify import is the largest distinct live list timed here.
- macOS and Linux builds: only Windows x64 is packaged here.
- Long-duration soak tests beyond the idle measurement in `PERFORMANCE.md`.
