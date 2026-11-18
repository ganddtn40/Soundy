# Contributing to Soundy

Soundy is a fork of [Sonora](https://github.com/sonorahq/sonora). The fork keeps the native
GPUI player, the provider integrations and the upstream architecture, and changes the identity,
theme, packaging and runtime defaults. Read the upstream
[vision](https://sonorahq.org/docs/vision) and [roadmap](https://sonorahq.org/docs/roadmap) for
the parts that still apply. Open an issue before a large change, especially one that touches
provider behaviour or the renderer.

## What this fork accepts

- Fixes that keep the player lightweight: idle CPU, memory, startup and binary size.
- Real provider, playback, lyric and accessibility fixes that work for every platform the code
  already supports.
- Visual changes that stay within the emerald Liquid Glass direction described in the README.
- Documentation and packaging that keep the GPL and third-party attribution intact.

Soundy does not accept changes that bypass DRM, subscriptions, paywalls or a provider's terms;
that add placeholder features or remove working optional integrations; or that add telemetry,
self-updating or background polling without a measured reason.

## Building on Windows

Windows 11 x64 is the primary target. Install:

- Rust stable with the MSVC toolchain (`rustup default stable-x86_64-pc-windows-msvc`)
- Visual Studio Build Tools with the C++ workload, including CMake
- Inno Setup 6, only for the installer

Then, from a Developer PowerShell in the repository root:

```powershell
cargo build --release -p sonora
.\scripts\windows\build-soundy-installer.ps1
```

The executable is `target\release\soundy.exe`; the installer and its output land in `dist\`.
The crate is still named `sonora` because the fork keeps the upstream package layout; the
binary target is `soundy`.

Use Nix where available on Linux and macOS. The upstream dev shell has the native dependencies:

```sh
nix develop          # or: direnv allow
cargo run --locked --package sonora
```

Without Nix you need rustup, a C++ compiler, `cmake`, `pkg-config`, `mold`, and the development
packages for ALSA, D-Bus, fontconfig, libxkbcommon, libxcb, libX11 and SQLite. Running it also
needs a Vulkan driver. These are the upstream requirements; they are not packaged by this fork
yet.

For hot reload, the upstream workflow still works:

```sh
dx serve --hot-patch --package sonora --features hot-reload
```

## Before you open a pull request

Run at least:

```sh
cargo fmt
cargo test -p music -p state -p cli -p ui
cargo clippy --workspace --all-targets
```

If the change affects the theme or the renderer, run the release build and launch it, not only
the tests. If it affects idle work, record the numbers before and after with
`scripts/measure-soundy.ps1` and update `PERFORMANCE.md` with the real values. Do not claim a
memory, CPU or startup figure that was not measured.

Open pull requests against the `soundy` branch. Commits follow
[Conventional Commits](https://www.conventionalcommits.org/) with no body, and user-facing
changes should be mentioned under `## [Unreleased]` in `CHANGELOG.md` without implementation
detail.

## Translations

English must be present for every key. Other locales can lag behind, since a missing key falls
back to English. Soundy's name and its user-facing paths are not translated; provider names stay
as the provider spells them. Run `scripts/i18n-coverage.py` afterwards to refresh the README
table.

## Accessibility

Soundy must stay usable with screen readers. Every interactive element needs a stable id, an
AccessKit role, and an accessible name resolved from Fluent in `render`; icon-only buttons take
their name from their tooltip, and list rows expose position and size of set. Run
`cargo test -p ui --test a11y_tree` and extend it when you add a primitive. New composed screens
also need a manual pass with a screen reader before merge.

## AI

Use whatever tools you like, but you must understand, review and test what you submit. Pull
request descriptions, issues and discussion replies must be your own words. A generated summary
of a small change is fine, and so is AI proofreading or translation of text you wrote.
Contributions that ignore this may be closed without comment.

## License

Soundy is GPL-3.0-or-later, the same license as Sonora, and your contributions are licensed
under the same terms. Keep the copyright notices of the upstream authors. The Soundy name, icon
set and theme changes are additions of this fork.
