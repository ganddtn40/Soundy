<div align="center">

<img src="./.github/readme-icon.svg" alt="" width="60" height="60">

<h1>Soundy</h1>

### A lightweight native music player

</div>

Soundy is a Windows-first desktop music player written in Rust with
[GPUI](https://github.com/zed-industries/zed). It plays local files and streams from the
providers its upstream project supports, and it is built to stay small: no Electron, no
WebView shell, no client-side JavaScript framework, and no background polling beyond what
playback needs.

Soundy is a fork of [Sonora](https://github.com/sonorahq/sonora), which supplies the player,
provider integrations and the native UI toolkit. This fork changes the identity, the theme,
packaging and the runtime defaults, and keeps the provider behaviour and GPL obligations of
the project it comes from.

> [!IMPORTANT]
> **Soundy is not a piracy tool.**
>
> Soundy does not obtain, share, export or decrypt copyrighted material. It does not bypass
> DRM, subscriptions, paywalls or the rules of the services it connects to. If a provider
> requires a valid subscription to play a track, Soundy requires one too. Features aimed at
> ripping, downloading or redistributing protected streams are out of scope.

## What this fork changes

- **Soundy identity** - window title, tray, notifications, application id
  `app.soundy.player`, `soundy:` links, `soundy.exe`, icons and Windows installer.
- **Emerald Liquid Glass** - near-black graphite surfaces (`#090A0C` / `#101114`), emerald
  accents, frosted controls and the fullscreen player's ambient field. The fullscreen cover
  is presented as a vinyl record with a slow reflective sweep, over a blurred copy of the
  artwork.
- **No self-update** - a Soundy build is never replaced by a Sonora release. Update checking
  is off and the updater does not run.
- **No install reporting** - upstream's optional install ping is disabled; Soundy has no
  telemetry endpoint of its own.
- **Lower idle work** - no polling loops or timers are added; appearance defaults keep
  backdrop blur and motion available, and both can be turned off in Settings.

Provider names, service links and the Discord application id still belong to Sonora and its
providers. They are kept because they are the real services and credentials this player
uses; they are not claimed as Soundy-owned.

## Status

Windows 11 x64 is the primary target. The release build is tested locally; Linux and macOS
sources remain in place because they come from the same codebase, but they are not packaged
or released by this fork yet. There is no Soundy release server, package repository, AUR
package, Flatpak repository or code-signing certificate at this point.

## Building on Windows

Prerequisites:

- Rust stable with the MSVC toolchain
- Visual Studio Build Tools with the C++ workload (including CMake)
- Inno Setup 6, for the installer only

From a Developer PowerShell:

```powershell
cargo build --release -p sonora
```

The executable is `target\release\soundy.exe`. To build the installer:

```powershell
.\scripts\windows\build-soundy-installer.ps1
```

The installer and portable output land in `dist\`. The executable is unsigned; Windows may
show a SmartScreen warning for a locally built binary.

### Other platforms

The workspace keeps the Linux and macOS build paths. Use the upstream
[build documentation](https://sonorahq.org/docs) for platform packages, then run the same
`cargo build --release -p sonora` command. The resulting executable is named `soundy` on
Unix. The Flatpak manifest under `flatpak/` builds the Soundy binary and app id; it is a
source recipe, not a published repository.

## Data and logs

Soundy keeps its data in the platform's usual application directories under `soundy`:

- Windows: `%APPDATA%\soundy` and `%LOCALAPPDATA%\soundy`
- macOS: `~/Library/Application Support/soundy` and `~/Library/Caches/soundy`
- Linux: `$XDG_CONFIG_HOME/soundy` and `$XDG_CACHE_HOME/soundy`

`SOUNDY_LOG` sets the log filter. `SOUNDY_BLUR=0` turns backdrop blur off for a renderer that
cannot afford it. The legacy `SONORA_*` names are still read for settings that predate the
rename.

## Performance

Soundy targets an idle resident set below 100 MB and near-zero idle CPU. **The CPU goal is met;
the memory goal is not.** On Windows 11 x64 the release build idles at 107–115 MB resident
(97–102 MB private working set), with a median idle CPU of 0.00%, while closing the memory gap
would need a smaller renderer than GPUI's `wgpu` baseline. The measured figures, the method and the
caveats are in [PERFORMANCE.md](./PERFORMANCE.md). Nothing in this repository claims a measurement
that was not taken on a real run.

## Attribution and licence

Soundy is distributed under the **GNU General Public License v3.0 or later**, the same licence
as Sonora. See [COPYING](./COPYING) and [THIRD-PARTY.md](./THIRD-PARTY.md).

The original project and its authors are credited at
[sonorahq/sonora](https://github.com/sonorahq/sonora). The Soundy name, icon set and theme
changes are additions of this fork. The icon libraries, fonts and Rust crates bundled by the
upstream project keep their own licences; the third-party notices are included in the
installer and in the source tree.

Upstream credits that still apply:

- [Zed](https://github.com/zed-industries/zed) for GPUI
- [librespot](https://github.com/librespot-org/librespot) for Spotify playback
- [ytmusic-rs](https://github.com/sonorahq/ytmusic-rs) for YouTube Music
- The lyric, metadata and audio-decoding projects listed in [THIRD-PARTY.md](./THIRD-PARTY.md)

## Code signing

This fork has no code-signing certificate. Upstream Sonora's SignPath signing policy applies
to binaries built and signed by the Sonora project, not to builds made from this tree.
