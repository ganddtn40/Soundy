# Soundy performance

This document records only measurements taken on a real run of the release build. It is not a
projection. Re-run the commands below on your machine before comparing figures.

## Target

| Metric | Target | Why |
| --- | --- | --- |
| Idle resident memory | below 100 MB | The original brief for this fork |
| Idle CPU | as close to 0% as the renderer allows | No polling while nothing happens |
| Startup to usable window | as short as the platform allows | Native build, no WebView boot |
| Release executable size | small for a GPUI app | LTO, one codegen unit, stripped debug info |

## Method

1. Build with the release profile: `cargo build --release -p sonora`.
2. Start `target\release\soundy.exe` and leave it on the default screen without playing anything.
   Wait for the window and the first-run work to settle. Record the profile state with the
   numbers; a signed-in profile with imported playlists is not the same as a fresh one.
3. Measure the `soundy` process once a second and report the median, not the lowest sample. The
   table uses a 45 second window after a 15 second settle.
4. For playback, use a local file so no network buffering is involved, and set repeat one so the
   track stays active for the whole window.
5. Record the executable size with `Get-Item target\release\soundy.exe`.
6. Record startup as the time from `Start-Process` to the first non-zero `MainWindowHandle`, not
   from log timestamps.

The measurement script used for the table is in `scripts/measure-soundy.ps1`.

## Results

Measured on Windows 11 Home 25H2 (build 26200), x64, with the `soundy` 1.0.0 release executable
built from this tree on 18 November 2026. This is the final build, including the playlist importer.
The profile was **not fresh**: YouTube Music was signed in, five imported playlists were present
(one of them 1,000 rows) and a two-track local folder had been scanned. That is a realistic
profile, but it carries more state than the earlier fresh-profile runs.

Idle means the Home screen with nothing playing, sampled once a second for 45 seconds after a 15
second settle. Playback means the Songs screen playing a 30-second local MP3 with embedded album
art, followed by an 8-second local WAV, with repeat all on, sampled the same way. Startup is the
median of three runs from `Start-Process` to the first non-zero `MainWindowHandle`.

| Metric | Idle | Playback |
| --- | --- | --- |
| Median resident set (RSS) | 111.7 MB (45 samples, peak 111.7 MB) | 128.3 MB (peak 132.3 MB) |
| Median private working set | 100.0 MB | 102.4 MB |
| Median CPU | 0.00% | 0.13% |
| Startup to first window | 193 ms median (192–245 ms across 3 runs) | — |
| `soundy.exe` | 90,669,056 bytes (86.47 MiB) | — |

### Against the target

The brief asked for idle resident memory below 100 MB. **Soundy does not meet that target on
Windows 11: idle RSS measured 111.7 MB.** The number is reported as measured, not rounded down.

The private working set — the memory committed to the process rather than shared with DLLs and the
GPU driver — measured 100.0 MB, so it sits at the target rather than comfortably under it. The RSS
figure does not support a sub-100 MB claim today.

What the measurements do show:

- Idle CPU is effectively zero: every one of the 45 samples rounded to 0.00%. The app does not
  poll or redraw while nothing changes.
- Startup is fast: the window handle exists 192–245 ms after `CreateProcess` (median 193 ms), with
  no WebView2 process spawned for the default local/YouTube Music path.
- Playback adds about 17 MB of RSS and about 0.13% of one core for local files, mostly the decoded
  cover, the spectrum tap and the position updates. This is the cost with the cover on screen; the
  process falls back toward the idle figure once playback stops.
- Turning blur off (`{"appearance":{"blur":false,"blur_window":false}}`) changed median RSS by about
  1 MB (110.6 MB vs 109.8 MB in an earlier pair of runs) and changed CPU not at all, so backdrop
  blur is not what keeps the process above 100 MB.

The gap is the baseline cost of GPUI rendering through `wgpu` on DX12: the swapchain, the shader
and pipeline caches, and the GPU driver's allocations all count toward the process. Reaching
sub-100 MB RSS would need a smaller renderer or a different windowing stack, not tuning of Soundy's
own data structures. The honest summary is: CPU, startup and playback costs meet the brief; idle
memory misses the resident-set target by roughly 12 MB.

## Notes and limits

- GPUI renders through `wgpu`, and the GPU driver's memory is not part of the process's resident
  set. Compare the process's own RSS, not Task Manager's GPU tab.
- The first minute after launch includes provider and library initialization; the idle sample is
  taken after that work settles.
- A blurred backdrop and the rotating vinyl sweep only run in fullscreen while a track plays.
  They do not run on the default workspace screen or while playback is paused.
- No upstream baseline was captured before the fork was rebranded. The table measures Soundy
  itself; it does not claim an improvement over Sonora that was never measured.
