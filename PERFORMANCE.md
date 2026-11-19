# Soundy performance

This document records only measurements taken on a real run of the release build. It is not a
projection. Re-run the commands below on your machine before comparing figures.

## Target

| Metric | Target | Why |
| --- | --- | --- |
| Idle resident memory | below 100 MB | The original brief for this project |
| Idle CPU | as close to 0% as the renderer allows | No polling while nothing happens |
| Startup to usable window | as short as the platform allows | Native build, no WebView boot |
| Release executable size | small for a GPUI app | LTO, one codegen unit, stripped debug info |

## Method

1. Build with the release profile: `cargo build --release -p soundy`.
2. Start `target\release\soundy.exe`, leave the window restored at 936×648 and do not minimize it.
   A minimized window lets Windows trim the working set and understates idle memory.
3. Record the profile state with the numbers; a signed-in profile with imported playlists is not
   the same as a fresh one. Two profiles were measured: a fresh install with no account, no local
   folders and no imports, and the maintainer's populated profile with a signed-in YouTube Music
   session, two imported playlists (276 rows) and a small playback history.
4. Sample `Get-Process` once a second and report the settled figure, not the lowest sample. The
   tables below use a 60-second window after the window has settled.
5. For playback, play a provider stream for 60 seconds, then pause and sample again. Local-file
   playback was not measured because neither profile had a local folder configured.
6. Record the executable size with `Get-Item target\release\soundy.exe`.
7. Record startup as the time from `Start-Process` to the first non-zero `MainWindowHandle`, not
   from log timestamps. Poll with a 5 ms sleep and take the median of five runs.

The measurement script is in `scripts/measure-soundy.ps1`.

## Results

Measured on Windows 11 x64 on 19 November 2026 with the window restored at 936×648. Each run
settled for 45 seconds, then sampled `Get-Process` once a second for 60 seconds. Memory is the
median of those samples; CPU is the process's total processor time over the window.

Two binaries were compared: the shipped 1.0.0 baseline (`soundy.exe` from
`Soundy-1.0.0-Portable`, 90,892,800 bytes, SHA-256
`445AC4DCBEE3480C06365BCE66A04D2FA8CC58C4E68A8C3AD06B25D4A9A47B06`) and the 1.0.1 release
(`target\release\soundy.exe`, 90,958,848 bytes, SHA-256
`32E7ED68E39DEFACE41FED52CDFF09DA8A059B3F7D6FF832BB7BD31BD6DAC38A`). Each profile was measured
twice per binary, the second pair in reverse order, so drift over the session could not favour
one version.

| Profile | Build | Working set | Private commit | CPU over 60 s |
| --- | --- | ---: | ---: | ---: |
| Populated, pair 1 | 1.0.0 | 119.99 MB | 100.46 MB | 0.391 s |
| Populated, pair 1 | 1.0.1 | 119.35 MB | 99.96 MB | 0.531 s |
| Populated, pair 2 | 1.0.0 | 120.88 MB | 102.06 MB | 0.344 s |
| Populated, pair 2 | 1.0.1 | 120.81 MB | 101.48 MB | 0.344 s |
| Fresh, pair 1 | 1.0.0 | 107.89 MB | 99.05 MB | 0.328 s |
| Fresh, pair 1 | 1.0.1 | 104.00 MB | 87.92 MB | 0.406 s |
| Fresh, pair 2 | 1.0.0 | 103.88 MB | 87.88 MB | 0.562 s |
| Fresh, pair 2 | 1.0.1 | 104.33 MB | 89.89 MB | 0.297 s |

Startup to the first visible window, five cold launches each:

- 1.0.0: 298, 192, 186, 191, 202 ms (median 192 ms).
- 1.0.1: 198, 201, 203, 191, 202 ms (median 201 ms).

The populated profile is the maintainer's: a signed-in YouTube Music session, two imported
playlists (276 rows) and a small playback history. The fresh profile has no account, no local
folders and no imports. Memory differences between versions are no larger than the spread within
one version: 1.0.0's fresh private commit ranged 87.9–99.1 MB across its two runs, against
87.9–89.9 MB for 1.0.1, and the versions trade places in the populated pairs. CPU spans
0.30–0.56 s per idle minute on both versions with no ordering effect, which is redraw and
compositor noise rather than a new loop. The 1.0.1 executable is 66,048 bytes (0.07%) larger
because it carries the anonymous page reader and its dialog copy.

The working set is flat across the samples (±0.3 MB), which is the useful signal for a leak: no
growth appeared while idle. The settled difference between the two profiles, about 16 MB, is the
signed-in provider session and the imported library data held for instant navigation, not the
importer: `Import` is inactive and polling nothing between imports.

Streaming was measured on the earlier 1.0.0 build at 124.5 MB working / 107.6 MB private. The
playback path did not change in 1.0.1, so it was not re-measured and is not presented as a 1.0.1
figure.

### Against the target

The brief asked for idle memory below 100 MB "where realistic". The result depends on the
profile:

- **Fresh profile:** private commit is 87.9–89.9 MB, under the 100 MB target. Working set is
  104.0–104.3 MB, about 4 MB over the literal resident target.
- **Populated profile:** private commit is 99.96–102.06 MB and working set is
  119.35–120.88 MB, so both are above the target once a provider session and imported playlists
  are live.

The baseline cost is GPUI rendering through `wgpu` on DX12: the swapchain, shader and pipeline
caches, font and image atlases, and the GPU driver's allocations all count toward the process.
The remaining profile-dependent cost is the signed-in provider session and the imported library
data held for instant search and navigation. Reaching sub-100 MB resident in a populated profile
would need a smaller renderer or a different windowing stack, not tuning of Soundy's own data
structures.

CPU is close to the renderer's floor: 0.30–0.56 s per idle minute, or about 0.5–0.9% of one core,
not a polling loop. Startup is fast because there is no WebView2 process and no JavaScript boot
for the default local/provider path.

An earlier pre-release build (91,254,784 bytes) measured 110.1 MB working / 98.7 MB private on a
lighter profile with a guest session and one five-row imported playlist, with a 213 ms median
startup. A previous version of this document also named a 90,808,320-byte binary with SHA-256
`29FC0319…CB24`; that binary was not the published 1.0.0 asset. The figures above use the
published baseline. The older numbers are kept only so notes written against them are not
mistaken for current measurements.

## What keeps it bounded

### Artwork cache (`crates/ui/src/artwork.rs`)

- 16 MB byte budget and 256-item cap, with at least one screenful kept for scroll-back.
- At most 8 loads pending at once, 2 decoders, and a prefetched cover may occupy only half of the
  pending slots, so a cover on screen is never starved by off-screen prefetch.
- Decoded images are bounded to a 4096 px longest edge; large provider covers are resized down
  rather than held at full resolution.
- One palette per picture, capped at 4096 tint entries, so the fullscreen player's background
  colour does not require re-analysing artwork every frame.

Every screen shares this one cache: home shelves, search, recents, albums, artists, playlists,
the queue, the right panel, the player bar and the fullscreen player. A cover is decoded once.

### Import and enrichment (`crates/music/src/enrich.rs`, `collage.rs`)

- At most 160 album lookups and 120 track lookups per import, with 4 concurrent requests.
- 12-second request timeout and a 6 MB cap per downloaded cover.
- Rows already carrying provider artwork are not looked up again; remaining rows are grouped by
  album so one fetch can serve several tracks.
- A provider that refuses or throttles is disabled for the rest of that import by a circuit
  breaker, instead of retrying every row.
- The anonymous Spotify page reader makes one bounded request (20-second timeout) with at most
  two retries at exponential backoff, checks cancellation between steps, and never starts on its
  own; there is no background importer or polling.
- Collage sources are at most four distinct album pictures, and the 2×2/3/2/1 collage is written
  once as JPEG rather than kept as decoded bitmaps.

### Database (`crates/storage/src/lib.rs`)

Indexed query paths rather than table scans:

- `plays_scope_time` for recents and statistics windows.
- `playlist_tracks_order` for ordered playlist reads.
- `imported_playlists_source` (unique) for import identity and re-import.
- `imported_entries_order` for ordered imported rows.

The import writes in one transaction per playlist. The state database on this profile is small;
the artwork cache lives on disk under `%LOCALAPPDATA%\soundy\artwork`, and generated playlist
covers under `%LOCALAPPDATA%\soundy\playlist-covers`.

### Lists and screens

- Song, album, artist and import lists are virtualized: only visible rows are built and their
  covers requested.
- Screens are lazily constructed and reuse the shared `Genres` and provider entities; opening
  Discover does not re-fetch genres the search page already has.
- No polling timer runs when the app is idle. The CPU samples above are redraw and compositor
  work, not background requests.

## Reproducing

```powershell
# Start from a restored 936×648 window; never maximize with ShowWindow.
$p = Start-Process .\target\release\soundy.exe -PassThru
Start-Sleep -Seconds 60
$p.Refresh()
"working=$([math]::Round($p.WorkingSet64/1MB,1)) MB private=$([math]::Round($p.PrivateMemorySize64/1MB,1)) MB"

$before = $p.CPU
Start-Sleep -Seconds 60
$p.Refresh()
"cpu over 60 s = $([math]::Round($p.CPU - $before,3)) s"
```

For startup, poll `MainWindowHandle` after `Start-Process` with a 5 ms sleep and stop the clock
when it becomes non-zero.

## Notes and limits

- The profiles are realistic but the populated one is not a fresh-install minimum: it carries two
  imported playlists and a signed-in provider session.
- The measurements are process-level. Shared DLL pages and the GPU driver's own allocations are
  counted in the working set but not the private commit.
- No long-duration soak test beyond the 60-second windows is claimed here; the flat samples show
  no short-term growth, not a proof about multi-hour sessions.
- Figures are for this machine and this binary. Rebuilding with a different GPU driver, Windows
  build or feature set will move them.
