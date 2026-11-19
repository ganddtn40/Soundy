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

1. Build with the release profile: `cargo build --release -p sonora`.
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

Measured on Windows 11 x64 with the 1.0.0 release executable built from this tree on
19 November 2026 (`soundy.exe`, 90,808,320 bytes,
SHA-256 `29FC0319B999ECBAF878168135218DF6622C058D242DADB0A19638B15258CB24`).

| State | Working set | Private commit | CPU over 60 s |
| --- | ---: | ---: | ---: |
| Fresh profile, Home, settled 60 s | 104.0 MB | 87.8 MB | 0.374 s (≈0.6% of one core) |
| Populated profile, Home, settled 60 s | 119.9 MB | 100.9 MB | 0.405 s (≈0.7% of one core) |
| Populated profile, playing a YouTube Music stream | 124.5 MB | 107.6 MB | 0.933 s (≈1.6% of one core) |
| Paused after that stream | 124.5 MB | 107.5 MB | 0.218 s over 45 s (≈0.5% of one core) |

Startup to first visible window, five cold launches of the same binary: **213 ms, 188 ms,
181 ms, 205 ms, 197 ms** (median 197 ms). The window appears before the provider home request
resolves; the shelves fill in afterwards without blocking input.

The working set is flat across the samples (±0.3 MB), which is the useful signal for a leak: no
growth appeared while idle or while streaming. The settled difference between the two profiles,
about 16 MB, is the signed-in provider session, the two imported playlists and the populated Home
shelves; it is not the importer running, since the importer is inactive while idle.

### Against the target

The brief asked for idle memory below 100 MB "where realistic". The result depends on the
profile:

- **Fresh profile:** private commit is 87.8 MB, under the 100 MB target. Working set is 104.0 MB,
  about 4 MB over the literal resident target.
- **Populated profile:** private commit is 100.9 MB and working set is 119.9 MB, so both are
  above the target once a provider session and imported playlists are live. Streaming adds about
  5 MB working and 7 MB private on top of that.

The baseline cost is GPUI rendering through `wgpu` on DX12: the swapchain, shader and pipeline
caches, font and image atlases, and the GPU driver's allocations all count toward the process.
The remaining profile-dependent cost is the signed-in provider session and the imported library
data held for instant search and navigation. Reaching sub-100 MB resident in a populated profile
would need a smaller renderer or a different windowing stack, not tuning of Soundy's own data
structures.

CPU is close to the renderer's floor: about 0.6–0.7% of one core averaged over a minute while
idle, and about 1.6% while decoding and playing a stream, not a polling loop. Startup is fast
because there is no WebView2 process and no JavaScript boot for the default local/provider path.

An earlier build (91,254,784 bytes) measured 110.1 MB working / 98.7 MB private on a lighter
profile with a guest session and one five-row imported playlist, with a 213 ms median startup.
Those numbers are superseded by the table above; they are kept here only so older notes are not
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

- At most 160 album lookups and 80 track lookups per import, with 4 concurrent requests.
- 12-second request timeout and a 6 MB cap per downloaded cover.
- Rows already carrying provider artwork are not looked up again; remaining rows are grouped by
  album so one fetch can serve several tracks.
- A provider that refuses or throttles is disabled for the rest of that import by a circuit
  breaker, instead of retrying every row.
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

- The profile is realistic but not a fresh-install minimum: it carries one imported playlist and
  a guest provider session.
- The measurements are process-level. Shared DLL pages and the GPU driver's own allocations are
  counted in the working set but not the private commit.
- No long-duration soak test beyond the 60-second windows is claimed here; the flat samples show
  no short-term growth, not a proof about multi-hour sessions.
- Figures are for this machine and this binary. Rebuilding with a different GPU driver, Windows
  build or feature set will move them.
