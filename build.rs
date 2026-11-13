//! Turns the source assets into compact raw blobs that the app embeds, so the
//! binary needs no PNG/MP3/font decoders at runtime — only `miniz_oxide`.
//!
//! Outputs in `OUT_DIR`:
//! - `closed.bin`, `open.bin`: 400×400 RGB, left-delta filtered, deflated
//! - `icon.bin`: 64×64 RGBA, deflated (window icon)
//! - `glyphs.bin`: alpha masks for every title letter, deflated
//! - `sound.bin`: mono i16 little-endian PCM, deflated
//! - `assets.rs`: sizes and glyph metrics, `include!`d by `main.rs`

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontRef, ScaleFont};

const IMAGE_SIZE: u32 = 400;
const ICON_SIZE: u32 = 64;
const TITLE: &str = "popue!";
/// Glyphs are rasterised at this pixel size and scaled at runtime.
const GLYPH_PX: f32 = 96.0;

fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(data, 10)
}

/// Stores each byte as the difference from the same channel of the pixel to
/// its left, which makes smooth images compress far better.
fn delta_filter(data: &mut [u8], width: usize, channels: usize) {
    for row in data.chunks_mut(width * channels) {
        for i in (channels..row.len()).rev() {
            row[i] = row[i].wrapping_sub(row[i - channels]);
        }
    }
}

fn image_rgb(path: &Path) -> Vec<u8> {
    let img = image::open(path).unwrap().to_rgb8();
    assert_eq!(img.dimensions(), (IMAGE_SIZE, IMAGE_SIZE), "{}", path.display());
    let mut raw = img.into_raw();
    delta_filter(&mut raw, IMAGE_SIZE as usize, 3);
    deflate(&raw)
}

fn icon_rgba(path: &Path) -> Vec<u8> {
    let img = image::open(path).unwrap().resize_exact(
        ICON_SIZE,
        ICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    deflate(&img.to_rgba8().into_raw())
}

fn sound_pcm(path: &Path) -> (Vec<u8>, u32, usize) {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = Box::new(fs::File::open(path).unwrap());
    let mss = MediaSourceStream::new(file, Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(&Hint::new(), mss, FormatOptions::default(), MetadataOptions::default())
        .unwrap();
    let track = format.default_track(TrackType::Audio).unwrap();
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(
            track.codec_params.as_ref().unwrap().audio().unwrap(),
            &AudioDecoderOptions::default(),
        )
        .unwrap();

    let mut rate = 0;
    let mut mono = Vec::<i16>::new();
    let mut buf = Vec::<f32>::new();
    while let Some(packet) = format.next_packet().unwrap() {
        if packet.track_id != track_id {
            continue;
        }
        let Ok(audio) = decoder.decode(&packet) else { continue };
        rate = audio.spec().rate();
        let channels = audio.spec().channels().count();
        buf.resize(audio.samples_interleaved(), 0.0);
        audio.copy_to_slice_interleaved(&mut buf);
        // A short click sound: downmixing to mono halves the size for free.
        for frame in buf.chunks(channels) {
            let avg = frame.iter().sum::<f32>() / channels as f32;
            mono.push((avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
        }
    }
    let samples = mono.len();
    let bytes: Vec<u8> = mono.iter().flat_map(|s| s.to_le_bytes()).collect();
    (deflate(&bytes), rate, samples)
}

fn glyphs(path: &Path, rs: &mut String) -> Vec<u8> {
    let data = fs::read(path).unwrap();
    let font = FontRef::try_from_slice(&data).unwrap();
    let scaled = font.as_scaled(GLYPH_PX);

    let mut masks = Vec::new();
    writeln!(rs, "pub const GLYPH_PX: f32 = {GLYPH_PX:?};").unwrap();
    writeln!(rs, "pub const GLYPH_ASCENT: f32 = {:?};", scaled.ascent()).unwrap();
    writeln!(rs, "pub const GLYPH_DESCENT: f32 = {:?};", scaled.descent()).unwrap();
    writeln!(rs, "pub const GLYPHS: &[GlyphInfo] = &[").unwrap();
    for c in TITLE.chars() {
        let id = font.glyph_id(c);
        let advance = scaled.h_advance(id);
        let glyph = id.with_scale_and_position(GLYPH_PX, ab_glyph::point(0.0, 0.0));
        let outline = font.outline_glyph(glyph).expect("title letters have outlines");
        let bounds = outline.px_bounds();
        let (w, h) = (bounds.width() as usize, bounds.height() as usize);
        let mut mask = vec![0u8; w * h];
        outline.draw(|x, y, c| mask[y as usize * w + x as usize] = (c * 255.0).round() as u8);
        writeln!(
            rs,
            "    GlyphInfo {{ width: {w}, height: {h}, left: {:?}, top: {:?}, advance: {advance:?}, offset: {} }},",
            bounds.min.x,
            bounds.min.y,
            masks.len()
        )
        .unwrap();
        masks.extend_from_slice(&mask);
    }
    writeln!(rs, "];").unwrap();
    deflate(&masks)
}

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let assets = Path::new("assets");
    for name in ["closed.png", "open.png", "icon.png", "popue.mp3", "fonts/DejaVuSans-Bold.ttf"] {
        println!("cargo:rerun-if-changed=assets/{name}");
    }

    let mut rs = String::new();
    writeln!(rs, "pub const IMAGE_SIZE: usize = {IMAGE_SIZE};").unwrap();
    writeln!(rs, "pub const ICON_SIZE: u32 = {ICON_SIZE};").unwrap();

    fs::write(out.join("closed.bin"), image_rgb(&assets.join("closed.png"))).unwrap();
    fs::write(out.join("open.bin"), image_rgb(&assets.join("open.png"))).unwrap();
    fs::write(out.join("icon.bin"), icon_rgba(&assets.join("icon.png"))).unwrap();

    let (pcm, rate, samples) = sound_pcm(&assets.join("popue.mp3"));
    fs::write(out.join("sound.bin"), pcm).unwrap();
    writeln!(rs, "pub const SOUND_RATE: u32 = {rate};").unwrap();
    writeln!(rs, "pub const SOUND_SAMPLES: usize = {samples};").unwrap();

    let masks = glyphs(&assets.join("fonts/DejaVuSans-Bold.ttf"), &mut rs);
    fs::write(out.join("glyphs.bin"), masks).unwrap();

    fs::write(out.join("assets.rs"), rs).unwrap();

    #[cfg(windows)]
    windows_icon(&assets.join("icon.png"), &out);
}

/// Embeds the icon into the .exe so Explorer and the taskbar show it.
#[cfg(windows)]
fn windows_icon(path: &Path, out: &Path) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = out.join("icon.ico");
    let img = image::open(path).unwrap().resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    img.save_with_format(&ico, image::ImageFormat::Ico).unwrap();
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().unwrap());
    res.compile().unwrap();
}
