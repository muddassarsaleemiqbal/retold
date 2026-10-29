//! Deterministic RGBA shot composition and content-addressed `ProRes` 4444 intermediates.
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Process(#[from] retold_providers::ProviderError),
    #[error("{0}")]
    Invalid(String),
    #[error("cancelled")]
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    pub image: PathBuf,
    pub x: i32,
    pub y: i32,
    pub first_frame: u32,
    pub end_frame: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    pub image: PathBuf,
    /// Grayscale relative depth, near = white; S2 supplies real model output.
    pub depth: Option<PathBuf>,
    pub frames: u32,
    pub fps: u32,
    /// Total camera displacement in pixels, scaled per pixel by relative depth.
    pub motion_x: f32,
    pub motion_y: f32,
    #[serde(default)]
    pub sprites: Vec<Sprite>,
    /// Drawn into the artwork before camera motion, so they move with it (mouths, eyes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attached: Vec<Sprite>,
}
/// Sprites pasted into a source image before any camera or parallax transform.
#[derive(Debug, Clone, Default)]
pub struct Attachments {
    sprites: Vec<(Sprite, std::sync::Arc<RgbaImage>)>,
}
impl Shot {
    pub fn to_compositor(&self) -> Result<Compositor, Error> {
        if self.frames == 0
            || self.frames > 36_000
            || self.fps == 0
            || self.fps > 120
            || !self.motion_x.is_finite()
            || !self.motion_y.is_finite()
        {
            return Err(Error::Invalid(
                "shot requires 1–36000 frames, 1–120 fps and finite motion".into(),
            ));
        }
        let image = image::open(&self.image)?.into_rgba8();
        if image.width() == 0
            || image.height() == 0
            || u64::from(image.width()) * u64::from(image.height()) > 16_777_216
        {
            return Err(Error::Invalid("shot image dimensions exceed limit".into()));
        }
        let depth = self
            .depth
            .as_ref()
            .map(|p| image::open(p).map(image::DynamicImage::into_luma8))
            .transpose()?;
        if depth
            .as_ref()
            .is_some_and(|d| d.dimensions() != image.dimensions())
        {
            return Err(Error::Invalid("depth and image dimensions differ".into()));
        }
        let sprites = self
            .sprites
            .iter()
            .map(|s| {
                if !(s.first_frame < s.end_frame && (1..=self.frames).contains(&s.end_frame)) {
                    return Err(Error::Invalid("sprite timing outside shot".into()));
                }
                Ok((s.clone(), image::open(&s.image)?.into_rgba8()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let attached = Attachments::load(&self.attached, self.frames)?;
        Ok(Compositor {
            shot: self.clone(),
            image,
            depth,
            sprites,
            attached,
        })
    }
}

impl Attachments {
    pub fn load(sprites: &[Sprite], frames: u32) -> Result<Self, Error> {
        let mut cache: std::collections::BTreeMap<PathBuf, std::sync::Arc<RgbaImage>> =
            std::collections::BTreeMap::new();
        let mut loaded = Vec::with_capacity(sprites.len());
        for s in sprites {
            if !(s.first_frame < s.end_frame && (1..=frames).contains(&s.end_frame)) {
                return Err(Error::Invalid("attached sprite timing outside shot".into()));
            }
            let image = if let Some(image) = cache.get(&s.image) {
                std::sync::Arc::clone(image)
            } else {
                let image = std::sync::Arc::new(image::open(&s.image)?.into_rgba8());
                cache.insert(s.image.clone(), std::sync::Arc::clone(&image));
                image
            };
            loaded.push((s.clone(), image));
        }
        Ok(Self { sprites: loaded })
    }
    /// The source for `index`, or `None` when no attachment is active.
    #[must_use]
    pub fn apply(&self, base: &RgbaImage, index: u32) -> Option<RgbaImage> {
        let mut active = self
            .sprites
            .iter()
            .filter(|(s, _)| index >= s.first_frame && index < s.end_frame)
            .peekable();
        active.peek()?;
        let mut out = base.clone();
        for (sprite, image) in active {
            image::imageops::overlay(
                &mut out,
                image.as_ref(),
                i64::from(sprite.x),
                i64::from(sprite.y),
            );
        }
        Some(out)
    }
    /// Content identity of attached sprites, empty when there are none.
    pub fn key(sprites: &[Sprite]) -> Result<Vec<u8>, Error> {
        if sprites.is_empty() {
            return Ok(Vec::new());
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"attached-v1");
        // Paths are not rendering inputs; each sprite is identified by its pixels.
        let mut contents = std::collections::BTreeMap::new();
        for s in sprites {
            if !contents.contains_key(&s.image) {
                contents.insert(&s.image, blake3::hash(&std::fs::read(&s.image)?));
            }
            hash.update(contents[&s.image].as_bytes());
            hash.update(&serde_json::to_vec(&(
                s.x,
                s.y,
                s.first_frame,
                s.end_frame,
            ))?);
        }
        Ok(hash.finalize().as_bytes().to_vec())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rendered {
    pub key: String,
    pub path: PathBuf,
    pub frames: u32,
    pub fps: u32,
    pub frame_hashes: Vec<String>,
    pub clip_hash: String,
}
#[derive(Debug)]
pub struct Compositor {
    shot: Shot,
    image: RgbaImage,
    depth: Option<image::GrayImage>,
    sprites: Vec<(Sprite, RgbaImage)>,
    attached: Attachments,
}
impl Compositor {
    pub fn load(shot: &Shot) -> Result<Self, Error> {
        shot.to_compositor()
    }

    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        self.image.dimensions()
    }
    pub fn frame(&self, index: u32) -> Result<RgbaImage, Error> {
        if index >= self.shot.frames {
            return Err(Error::Invalid("frame outside shot".into()));
        }
        let progress = if self.shot.frames == 1 {
            0.0
        } else {
            f64::from(index) / f64::from(self.shot.frames - 1)
        };
        let source = self.attached.apply(&self.image, index);
        let source = source.as_ref().unwrap_or(&self.image);
        let mut out = RgbaImage::new(self.image.width(), self.image.height());
        for (x, y, pixel) in out.enumerate_pixels_mut() {
            let depth = self
                .depth
                .as_ref()
                .map_or(1.0, |d| f64::from(d.get_pixel(x, y)[0]) / 255.0);
            *pixel = sample(
                source,
                f64::from(x) - f64::from(self.shot.motion_x) * progress * depth,
                f64::from(y) - f64::from(self.shot.motion_y) * progress * depth,
            );
        }
        for (sprite, image) in &self.sprites {
            if index >= sprite.first_frame && index < sprite.end_frame {
                image::imageops::overlay(&mut out, image, i64::from(sprite.x), i64::from(sprite.y));
            }
        }
        Ok(out)
    }
}
// Interpolation is bounded by dimensions (<= 2^24 pixels); casts below follow clamping.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "coordinates/channels are explicitly clamped to image/u8 bounds"
)]
fn sample(image: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    if x < 0.0 || y < 0.0 || x > f64::from(image.width() - 1) || y > f64::from(image.height() - 1) {
        return Rgba([0, 0, 0, 0]);
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let fx = x - f64::from(x0);
    let fy = y - f64::from(y0);
    let weights = [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ];
    let mut result = [0.0; 4];
    for (x, y, w) in weights {
        let p = image.get_pixel(x, y);
        let a = f64::from(p[3]) / 255.0;
        for c in 0..3 {
            result[c] += f64::from(p[c]) * a * w;
        }
        result[3] += a * w;
    }
    if result[3] > 0.0 {
        for c in 0..3 {
            result[c] /= result[3];
        }
    }
    Rgba([
        result[0].round() as u8,
        result[1].round() as u8,
        result[2].round() as u8,
        (result[3] * 255.0).round() as u8,
    ])
}
pub fn content_key(shot: &Shot) -> Result<String, Error> {
    let mut hash = blake3::Hasher::new();
    hash.update(b"rgba-shot-prores-v1");
    // Paths are not rendering inputs: identical assets in another folder should reuse clips.
    hash.update(&serde_json::to_vec(&(
        shot.frames,
        shot.fps,
        shot.motion_x,
        shot.motion_y,
    ))?);
    for path in std::iter::once(&shot.image)
        .chain(shot.depth.iter())
        .chain(shot.sprites.iter().map(|s| &s.image))
    {
        hash.update(&std::fs::read(path)?);
    }
    hash.update(&serde_json::to_vec(
        &shot
            .sprites
            .iter()
            .map(|s| (s.x, s.y, s.first_frame, s.end_frame))
            .collect::<Vec<_>>(),
    )?);
    hash.update(&Attachments::key(&shot.attached)?);
    Ok(hash.finalize().to_hex().to_string())
}
pub fn ffmpeg() -> PathBuf {
    std::env::var_os("RETOLD_FFMPEG").map_or_else(|| PathBuf::from("ffmpeg"), PathBuf::from)
}
pub async fn version(cancel: &CancellationToken) -> Result<String, Error> {
    let cwd = std::env::current_dir()?;
    let limits = retold_providers::process::Limits {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let output =
        retold_providers::process::run(&ffmpeg(), &["-version".into()], "", &cwd, &limits, cancel)
            .await?;
    if output.status != Some(0) {
        return Err(Error::Invalid(
            "ffmpeg version check failed; set RETOLD_FFMPEG".into(),
        ));
    }
    Ok(output.stdout.lines().next().unwrap_or_default().into())
}
pub async fn render(
    shot: &Shot,
    cache: &Path,
    cancel: &CancellationToken,
) -> Result<Rendered, Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let compositor = Compositor::load(shot)?;
    let key = content_key(shot)?;
    std::fs::create_dir_all(cache)?;
    let mut lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.join(format!("{key}.lock")))?;
    // This short-lived async handle never waits behind another renderer while holding a run lock.
    lock.try_lock()
        .map_err(|e| Error::Invalid(format!("shot render already running: {e}")))?;
    std::io::Write::flush(&mut lock)?;
    let metadata = cache.join(format!("{key}.json"));
    if let Ok(bytes) = std::fs::read(&metadata) {
        let saved: Rendered = serde_json::from_slice(&bytes)?;
        if saved.key == key
            && std::fs::read(&saved.path)
                .is_ok_and(|b| blake3::hash(&b).to_hex().as_str() == saved.clip_hash)
        {
            return Ok(saved);
        }
    }
    let destination = cache.join(format!("{key}.mov"));
    let temporary = tempfile::NamedTempFile::new_in(cache)?;
    let (width, height) = compositor.dimensions();
    let frames = shot.frames;
    let frame_hashes = encode_frames(
        (0..frames).map(move |index| compositor.frame(index)),
        (width, height),
        shot.fps,
        temporary.path(),
        cancel,
    )
    .await?;
    let clip_hash = blake3::hash(&std::fs::read(temporary.path())?)
        .to_hex()
        .to_string();
    temporary.as_file().sync_all()?;
    temporary.persist(&destination).map_err(|e| e.error)?;
    let result = Rendered {
        key,
        path: destination,
        frames: shot.frames,
        fps: shot.fps,
        frame_hashes,
        clip_hash,
    };
    let mut meta = tempfile::NamedTempFile::new_in(cache)?;
    std::io::Write::write_all(&mut meta, &serde_json::to_vec_pretty(&result)?)?;
    meta.as_file().sync_all()?;
    meta.persist(metadata).map_err(|e| e.error)?;
    std::fs::File::open(cache)?.sync_all()?;
    Ok(result)
}
/// PNG sequence is retained as a lossless fallback for editor installations without `ProRes`.
pub fn png_sequence(
    shot: &Shot,
    directory: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<String>, Error> {
    let compositor = Compositor::load(shot)?;
    std::fs::create_dir_all(directory)?;
    let mut hashes = Vec::new();
    for index in 0..shot.frames {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let frame = compositor.frame(index)?;
        hashes.push(blake3::hash(frame.as_raw()).to_hex().to_string());
        frame.save(directory.join(format!("{index:06}.png")))?;
    }
    Ok(hashes)
}

fn encoder_args(width: u32, height: u32, fps: u32, destination: &Path) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pixel_format".into(),
        "rgba".into(),
        "-video_size".into(),
        format!("{width}x{height}"),
        "-framerate".into(),
        fps.to_string(),
        "-i".into(),
        "pipe:0".into(),
        "-an".into(),
        "-c:v".into(),
        "prores_ks".into(),
        "-profile:v".into(),
        "4".into(),
        "-pix_fmt".into(),
        "yuva444p10le".into(),
        "-alpha_bits".into(),
        "16".into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "mov".into(),
        destination.display().to_string(),
    ]
}

/// Streams a deterministic frame iterator through the same bounded encoder and process group.
pub async fn encode_frames(
    frames: impl Iterator<Item = Result<RgbaImage, Error>> + Send + 'static,
    dimensions: (u32, u32),
    fps: u32,
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<String>, Error> {
    let (width, height) = dimensions;
    let args = encoder_args(width, height, fps, destination);
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let token = cancel.clone();
    let producer = tokio::task::spawn_blocking(move || {
        let mut hashes = Vec::new();
        for frame in frames {
            if token.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let frame = frame?;
            if frame.dimensions() != dimensions {
                return Err(Error::Invalid("encoded frame dimensions changed".into()));
            }
            let bytes = frame.into_raw();
            hashes.push(blake3::hash(&bytes).to_hex().to_string());
            sender
                .blocking_send(bytes)
                .map_err(|e| Error::Invalid(e.to_string()))?;
        }
        Ok::<_, Error>(hashes)
    });
    let limits = retold_providers::process::Limits {
        timeout: Duration::from_secs(1800),
        ..Default::default()
    };
    let output = retold_providers::process::run_stream(
        &ffmpeg(),
        &args,
        &[],
        receiver,
        destination.parent().unwrap_or_else(|| Path::new(".")),
        &limits,
        cancel,
    )
    .await;
    let hashes = producer.await.map_err(|e| Error::Invalid(e.to_string()))?;
    let output = output?;
    if output.status != Some(0) {
        return Err(Error::Invalid(format!(
            "ffmpeg shot encoding failed: {}",
            output.stderr
        )));
    }
    hashes
}
