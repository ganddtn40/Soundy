//! A playlist cover made from the artwork of its own tracks.
//!
//! Streaming services draw a playlist with a grid of four album covers. An imported playlist
//! has no such picture, so this module builds one: four tiles in a 2×2 grid, or a layout for
//! two or three covers, or a designed emerald card when the list has no artwork at all. The
//! result is a JPEG of a fixed side length, which the local client writes through its own
//! square-cover path, so the playlist and the library use one picture for good.

use std::io::Cursor;

use anyhow::{Context as _, Result, anyhow};
use image::{DynamicImage, ExtendedColorType, ImageEncoder as _, Rgba, RgbaImage, imageops};

use crate::ImportedEntry;
use crate::import::key;

/// The side of the finished cover. It is larger than the player ever shows it and small enough
/// that four decoded covers and the canvas fit in a few megabytes.
pub const SIDE: u32 = 640;
const TILE: u32 = SIDE / 2;
/// A source picture larger than this on either side is refused rather than decoded, so a
/// mistaken 12,000-pixel scan cannot allocate hundreds of megabytes during an import.
const MAX_SOURCE: u32 = 4000;

/// Builds a playlist cover from up to four covers, each already downloaded. Pictures that do
/// not decode are skipped; if none decodes the caller keeps its placeholder instead of getting
/// a blank card. The layout uses every picture it is given.
pub fn collage(covers: &[Vec<u8>]) -> Result<Vec<u8>> {
    let tiles: Vec<DynamicImage> = covers
        .iter()
        .take(4)
        .filter_map(|bytes| decode(bytes))
        .collect();
    let first = tiles
        .first()
        .ok_or_else(|| anyhow!("none of the covers could be decoded"))?;
    let canvas = match tiles.len() {
        1 => fill(first, SIDE, SIDE),
        n => {
            let mut canvas = RgbaImage::new(SIDE, SIDE);
            match n {
                2 => {
                    place(&mut canvas, &tiles[0], 0, 0, TILE, SIDE);
                    place(&mut canvas, &tiles[1], TILE, 0, TILE, SIDE);
                }
                3 => {
                    place(&mut canvas, &tiles[0], 0, 0, TILE, SIDE);
                    place(&mut canvas, &tiles[1], TILE, 0, TILE, TILE);
                    place(&mut canvas, &tiles[2], TILE, TILE, TILE, TILE);
                }
                _ => {
                    place(&mut canvas, &tiles[0], 0, 0, TILE, TILE);
                    place(&mut canvas, &tiles[1], TILE, 0, TILE, TILE);
                    place(&mut canvas, &tiles[2], 0, TILE, TILE, TILE);
                    place(&mut canvas, &tiles[3], TILE, TILE, TILE, TILE);
                }
            }
            seam(&mut canvas);
            canvas
        }
    };
    encode(&canvas)
}

/// A designed cover for a playlist with no usable artwork. The seed (the playlist name or its
/// source id) chooses a hue inside the emerald family and a ring rotation, so two lists do not
/// get identical cards while both still belong to the app's palette. It is drawn, not
/// borrowed: there is no picture here to mis-attribute.
pub fn fallback(seed: &str) -> Vec<u8> {
    let hash = fnv(seed.as_bytes());
    let hue = 148.0 + (hash % 26) as f32;
    let turn = (hash >> 8) % 360;
    let light = mix(emerald(hue, 0.32, 0.16), emerald(hue, 0.52, 0.34), 0.55);

    let mut canvas = RgbaImage::new(SIDE, SIDE);
    let side = SIDE as f32;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let fx = x as f32 / side;
            let fy = y as f32 / side;
            // A diagonal wash, then a soft light from the upper left and a vignette.
            let wash = (fx * 0.55 + fy * 0.45).clamp(0., 1.);
            let mut color = mix(emerald(hue, 0.30, 0.13), emerald(hue, 0.46, 0.30), wash);
            let glow = (1. - ((fx - 0.30).powi(2) + (fy - 0.22).powi(2)).sqrt() / 0.85).max(0.);
            color = plus(
                color,
                mix(emerald(hue, 0.58, 0.42), light, 0.4),
                glow * 0.22,
            );
            let edge = (fx - 0.5).abs().max((fy - 0.5).abs()) * 2.;
            color = minus(color, 0.22 * edge.powf(2.6));

            // Concentric grooves, the quiet sign of a record without drawing one literally.
            let (cx, cy) = (fx - 0.5, fy - 0.53);
            let radius = (cx * cx + cy * cy).sqrt();
            let angle = (cy.atan2(cx).to_degrees() + turn as f32).to_radians();
            let ring = (radius * 4.6 + angle * 0.018).fract();
            if (0.28..0.47).contains(&radius) && ring < 0.028 {
                color = plus(color, light, 0.10);
            }

            canvas.put_pixel(x, y, pixel(color));
        }
    }

    // A note in the middle: an oval head, a stem and the flag that curls from its top.
    let ink = Rgba([236, 252, 243, 236]);
    let (hx, hy, rx, ry) = (250.0_f32, 432.0, 47.0, 35.0);
    let tilt = -0.32_f32;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let (dx, dy) = (x as f32 - hx, y as f32 - hy);
            let (lx, ly) = (
                dx * tilt.cos() + dy * tilt.sin(),
                -dx * tilt.sin() + dy * tilt.cos(),
            );
            let head = (lx / rx).powi(2) + (ly / ry).powi(2) <= 1.;
            let stem =
                (292.0..=306.0).contains(&(x as f32)) && (238.0..=432.0).contains(&(y as f32));
            let flag = flag(x as f32, y as f32);
            if head || stem || flag {
                canvas.put_pixel(x, y, ink);
            }
        }
    }

    // A small second note, so the card reads as music rather than a logo.
    let (hx, hy, rx, ry) = (400.0_f32, 470.0, 22.0, 16.0);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let head = ((x as f32 - hx) / rx).powi(2) + ((y as f32 - hy) / ry).powi(2) <= 1.;
            let stem =
                (418.0..=426.0).contains(&(x as f32)) && (392.0..=470.0).contains(&(y as f32));
            if head || stem {
                canvas.put_pixel(x, y, Rgba([236, 252, 243, 178]));
            }
        }
    }

    encode(&canvas).unwrap_or_default()
}

/// The curved flag of the drawn note: a thick arc around a centre above the stem.
fn flag(x: f32, y: f32) -> bool {
    let (cx, cy, radius) = (306.0_f32, 330.0, 84.0);
    let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let angle = (y - cy).atan2(x - cx).to_degrees();
    (radius - 8.0..=radius + 8.0).contains(&distance) && (-88.0..=-18.0).contains(&angle)
}

/// The distinct artwork a collage can use: one picture per album, in source order, and never
/// the same URL twice. A row with no picture, or one that is not an HTTP cover, is skipped.
/// Four are enough for any layout and bound what one import downloads.
pub fn sources(entries: &[ImportedEntry]) -> Vec<String> {
    let mut albums = std::collections::HashSet::new();
    let mut urls = std::collections::HashSet::new();
    let mut found = Vec::new();
    for entry in entries {
        let Some(cover) = entry.cover.as_deref() else {
            continue;
        };
        if !cover.starts_with("https://") && !cover.starts_with("http://") {
            continue;
        }
        let album = key(&entry.album);
        if !album.is_empty() && !albums.insert(album) {
            continue;
        }
        if urls.insert(cover.to_owned()) {
            found.push(cover.to_owned());
        }
        if found.len() == 4 {
            break;
        }
    }
    found
}

fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > MAX_SOURCE || height > MAX_SOURCE {
        return None;
    }
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

fn fill(image: &DynamicImage, width: u32, height: u32) -> RgbaImage {
    image
        .resize_to_fill(width, height, imageops::FilterType::Lanczos3)
        .to_rgba8()
}

fn place(canvas: &mut RgbaImage, image: &DynamicImage, x: u32, y: u32, width: u32, height: u32) {
    imageops::replace(
        canvas,
        &fill(image, width, height),
        i64::from(x),
        i64::from(y),
    );
}

/// A hairline between tiles so four covers read as a grid rather than one noisy picture.
fn seam(canvas: &mut RgbaImage) {
    let line = Rgba([8, 14, 12, 92]);
    for offset in 0..SIDE {
        for thickness in 0..2 {
            let near = TILE + thickness - 1;
            canvas.put_pixel(near, offset, line);
            canvas.put_pixel(offset, near, line);
        }
    }
}

fn encode(canvas: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let (width, height) = canvas.dimensions();
    // Drop the alpha channel: a JPEG has none, and the canvas is opaque anyway.
    let raw = canvas.as_raw();
    let mut opaque = Vec::with_capacity((width * height * 3) as usize);
    for pixel in raw.chunks_exact(4) {
        opaque.extend_from_slice(&pixel[..3]);
    }
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
    encoder
        .write_image(&opaque, width, height, ExtendedColorType::Rgb8)
        .context("cannot encode the playlist cover")?;
    Ok(out)
}

/// A multiply-add light: `base + tint * amount`, clamped.
fn plus(base: [f32; 3], tint: [f32; 3], amount: f32) -> [f32; 3] {
    [
        (base[0] + tint[0] * amount).clamp(0., 1.),
        (base[1] + tint[1] * amount).clamp(0., 1.),
        (base[2] + tint[2] * amount).clamp(0., 1.),
    ]
}

fn minus(base: [f32; 3], amount: f32) -> [f32; 3] {
    [
        (base[0] * (1. - amount)).clamp(0., 1.),
        (base[1] * (1. - amount)).clamp(0., 1.),
        (base[2] * (1. - amount)).clamp(0., 1.),
    ]
}

fn mix(left: [f32; 3], right: [f32; 3], amount: f32) -> [f32; 3] {
    let amount = amount.clamp(0., 1.);
    [
        left[0] + (right[0] - left[0]) * amount,
        left[1] + (right[1] - left[1]) * amount,
        left[2] + (right[2] - left[2]) * amount,
    ]
}

fn pixel(color: [f32; 3]) -> Rgba<u8> {
    Rgba([
        (color[0] * 255.).round().clamp(0., 255.) as u8,
        (color[1] * 255.).round().clamp(0., 255.) as u8,
        (color[2] * 255.).round().clamp(0., 255.) as u8,
        255,
    ])
}

/// A colour in the emerald family from a hue, saturation and lightness, in HSL.
fn emerald(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let c = (1. - (2. * lightness - 1.).abs()) * saturation;
    let h = (hue.rem_euclid(360.)) / 60.;
    let x = c * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = lightness - c / 2.;
    [r + m, g + m, b + m]
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(color: [u8; 3]) -> Vec<u8> {
        let image = RgbaImage::from_pixel(TILE, TILE, Rgba([color[0], color[1], color[2], 255]));
        encode(&image).unwrap()
    }

    #[test]
    fn four_covers_become_a_four_tile_jpeg() {
        let covers = [
            solid([220, 40, 40]),
            solid([40, 180, 90]),
            solid([40, 80, 220]),
            solid([230, 200, 60]),
        ];
        let bytes = collage(&covers).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (SIDE, SIDE));
        // Each quadrant keeps its own cover.
        let red = image.to_rgba8().get_pixel(80, 80).0;
        let green = image.to_rgba8().get_pixel(560, 80).0;
        let blue = image.to_rgba8().get_pixel(80, 560).0;
        assert!(red[0] > green[0] && red[0] > blue[0]);
        assert!(green[1] > red[1] && green[1] > blue[1]);
        assert!(blue[2] > red[2] && blue[2] > green[2]);
    }

    #[test]
    fn one_cover_fills_the_whole_cover() {
        let bytes = collage(&[solid([200, 60, 60])]).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (SIDE, SIDE));
        let corner = image.to_rgba8().get_pixel(20, 20).0;
        assert!(corner[0] > 150);
    }

    #[test]
    fn unusable_covers_are_refused_rather_than_drawn_blank() {
        assert!(collage(&[b"not an image".to_vec()]).is_err());
        assert!(collage(&[]).is_err());
    }

    #[test]
    fn the_fallback_is_a_drawn_emerald_card_that_differs_by_name() {
        let one = fallback("Late night drive");
        let image = image::load_from_memory(&one).unwrap();
        assert_eq!((image.width(), image.height()), (SIDE, SIDE));
        // The card has both a dark background and bright ink, not one flat colour.
        let rgba = image.to_rgba8();
        let dark = rgba.get_pixel(40, 40).0;
        let ink = rgba.get_pixel(250, 430).0;
        assert!(dark[1] > dark[0], "the background leans green");
        assert!(ink[0] > 180 && ink[1] > 180);
        assert_ne!(one, fallback("Morning coffee"));
    }

    #[test]
    fn the_sources_take_one_picture_per_album_and_stop_at_four() {
        let mut entries = Vec::new();
        for (position, (album, cover)) in [
            ("One", "https://art.example/one.jpg"),
            ("One", "https://art.example/one-other.jpg"),
            ("Two", "https://art.example/two.jpg"),
            ("Three", "https://art.example/three.jpg"),
            ("Four", "https://art.example/four.jpg"),
            ("Five", "https://art.example/five.jpg"),
        ]
        .into_iter()
        .enumerate()
        {
            entries.push(ImportedEntry {
                position: position as u32,
                album: album.to_owned(),
                cover: Some(cover.to_owned()),
                ..Default::default()
            });
        }
        entries.push(ImportedEntry {
            position: 6,
            album: "Six".to_owned(),
            cover: None,
            ..Default::default()
        });
        assert_eq!(
            sources(&entries),
            [
                "https://art.example/one.jpg",
                "https://art.example/two.jpg",
                "https://art.example/three.jpg",
                "https://art.example/four.jpg",
            ]
        );
    }
}
