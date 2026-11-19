# Soundy icon masters

The Soundy mark is a dark rounded glass tile with a five-bar emerald waveform: a centre bar and
two mirrored pairs, on a soft emerald halo.

| File | What it is |
| --- | --- |
| `scripts/generate-soundy-icons.py` | Draws and writes the whole set with Pillow. Run it from the repository root. |
| `soundy.svg` | Vector master, 512×512. Gradients, halo and tile edge are part of this file. |
| `soundy-1024.png` | 1024×1024 raster master, rendered from the same geometry at 4× and downsampled. |
| `soundy-512.png` | 512×512 preview, derived from the raster master. |

## Derived assets

| Asset | Source |
| --- | --- |
| `assets/windows/soundy.ico` | Raster master, sizes 16–256 |
| `assets/macos/soundy.icns`, `assets/macos/soundy-1024.png` | Raster master, 16–1024 |
| `assets/linux/soundy.svg` | Vector master, copied for Linux packaging |
| `assets/linux/icons/hicolor/*/apps/soundy.png` | Raster master, 16–512 |
| `assets/tray/template-18.png`, `template-36.png` | The waveform alone, black with alpha, for macOS template images |
| `.github/readme-icon.svg` | Vector master, used by the README header |

The Windows installer and the executable both embed `assets/windows/soundy.ico`; `crates/soundy/build.rs`
points the resource compiler at it, and `scripts/windows/soundy.iss` uses it for Setup.

## Palette

| Role | Value |
| --- | --- |
| Tile top | `#0b0d0f` |
| Tile bottom | `#14181a` |
| Waveform top | `#6ee7b7` |
| Waveform bottom | `#059669` |
| Halo | `#10b981` at 45% fading to transparent |
| Tile edge | `#ffffff` at 10% |

Keep new sizes on the same grid: bars are 44/512 wide with a 22/512 corner radius, spaced 80/512
apart, and the tile corner radius is 112/512.
