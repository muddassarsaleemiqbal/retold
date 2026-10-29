//! Content-addressed intermediates with full-frame QA and bounded gentler/flat recovery.
use crate::{
    Error,
    camera::CameraMove,
    depth::DepthMap,
    dibr::{self, Audit},
    inpaint,
};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub const VERSION: &str = "parallax-border-safe-bounded-fill-v2";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub image: PathBuf,
    pub depth: PathBuf,
    pub background: Option<PathBuf>,
    pub motion: CameraMove,
    pub strength: f64,
    pub frames: u32,
    pub fps: u32,
    /// Drawn into the scene before the camera move, so they ride with the artwork.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attached: Vec<retold_render::Sprite>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    Requested,
    Gentler,
    Flat,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rendered {
    pub key: String,
    pub path: PathBuf,
    pub clip_hash: String,
    pub frame_hashes: Vec<String>,
    pub fallback: Fallback,
    pub applied_strength: f64,
    pub audit: Audit,
    pub rejected: Vec<Audit>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Quality {
    pub max_hole_ratio: f64,
    pub max_stretched_fraction: f64,
}
impl Default for Quality {
    fn default() -> Self {
        Self {
            max_hole_ratio: 0.08,
            max_stretched_fraction: 0.01,
        }
    }
}
impl Audit {
    #[must_use]
    pub fn passes(&self, quality: Quality) -> bool {
        self.hole_ratio.is_finite()
            && self.stretched_fraction.is_finite()
            && (0.0..=quality.max_hole_ratio).contains(&self.hole_ratio)
            && (0.0..=quality.max_stretched_fraction).contains(&self.stretched_fraction)
            && self.unresolved_holes == 0
            && self.framing_scale.is_finite()
            && (0.0..=1.2).contains(&self.framing_scale)
            && (0.0..=0.004).contains(&self.fill_radius_ratio)
    }
}

impl Quality {
    #[must_use]
    pub fn passes(self, audit: Audit) -> bool {
        audit.passes(self)
    }
}
#[derive(Debug, Clone)]
pub struct Scene {
    image: Arc<RgbaImage>,
    depth: Arc<DepthMap>,
    background: Option<Arc<RgbaImage>>,
    attached: Arc<retold_render::Attachments>,
}
impl Scene {
    pub fn load(request: &Request) -> Result<Self, Error> {
        load_scene_from_request(request)
    }

    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        self.image.dimensions()
    }
    pub fn frame(
        &self,
        motion: CameraMove,
        progress: f64,
        strength: f64,
    ) -> Result<(RgbaImage, Audit), Error> {
        self.frame_from(&self.image, motion, progress, strength)
    }
    /// Frame `index` with any attached sprites drawn into the scene first.
    pub fn frame_at(
        &self,
        index: u32,
        motion: CameraMove,
        progress: f64,
        strength: f64,
    ) -> Result<(RgbaImage, Audit), Error> {
        match self.attached.apply(&self.image, index) {
            Some(source) => self.frame_from(&source, motion, progress, strength),
            None => self.frame(motion, progress, strength),
        }
    }
    fn frame_from(
        &self,
        image: &RgbaImage,
        motion: CameraMove,
        progress: f64,
        strength: f64,
    ) -> Result<(RgbaImage, Audit), Error> {
        if strength.abs() <= f64::EPSILON {
            return Ok((
                image.clone(),
                Audit {
                    max_stretch: 1.0,
                    ..Audit::default()
                },
            ));
        }
        let framing = framing_scale(&self.depth, motion, strength);
        let mut pose = motion.pose(progress, strength);
        pose.focal_scale *= framing.min(1.2);
        let mut warped = dibr::warp(image, &self.depth, pose)?;
        if let Some(background) = &self.background {
            let far = DepthMap::new(
                self.depth.width,
                self.depth.height,
                vec![0.0; self.depth.values.len()],
            )?;
            let plate = dibr::warp(background, &far, pose)?;
            for (i, hole) in warped.holes.iter_mut().enumerate() {
                if *hole && plate.image.as_raw()[i * 4 + 3] > 0 {
                    warped.image.as_mut()[i * 4..i * 4 + 4]
                        .copy_from_slice(&plate.image.as_raw()[i * 4..i * 4 + 4]);
                    warped.depths[i] = 0.0;
                    *hole = false;
                }
            }
        }
        // Record exposed border pixels before bounded reconstruction. Isolated splat
        // gaps may be filled, but broad strips fail the fill-radius gate.
        warped.audit.framing_scale = framing;
        let (w, h) = image.dimensions();
        warped.audit.edge_holes = u32::try_from(
            warped
                .holes
                .iter()
                .enumerate()
                .filter(|(i, hole)| {
                    **hole
                        && (i % w as usize == 0
                            || i % w as usize == w as usize - 1
                            || *i < w as usize
                            || *i >= (h as usize - 1) * w as usize)
                })
                .count(),
        )
        .unwrap_or(u32::MAX);
        inpaint::fill(&mut warped);
        Ok((warped.image, warped.audit))
    }
}
// One conservative margin for the whole move prevents frame-to-frame crop pumping.
// Border samples use real depth; the 20% cap is enforced by QA, then gentler/flat recovery.
fn framing_scale(depth: &DepthMap, motion: CameraMove, strength: f64) -> f64 {
    let size = (depth.width, depth.height);
    let cx = f64::from(size.0.saturating_sub(1)) / 2.0;
    let cy = f64::from(size.1.saturating_sub(1)) / 2.0;
    if cx <= f64::EPSILON || cy <= f64::EPSILON {
        return 1.0;
    }
    let mut scale = 1.0_f64;
    for step in 0..=8 {
        let pose = motion.pose(f64::from(step) / 8.0, strength);
        for y in 0..size.1 {
            for x in [0, size.0 - 1] {
                let p = crate::camera::project(x, y, depth.at(x, y), size, pose);
                let distance = if x == 0 { cx - p.0 } else { p.0 - cx };
                scale = scale.max((cx + 1.0) / distance.max(0.001));
            }
        }
        for x in 0..size.0 {
            for y in [0, size.1 - 1] {
                let p = crate::camera::project(x, y, depth.at(x, y), size, pose);
                let distance = if y == 0 { cy - p.1 } else { p.1 - cy };
                scale = scale.max((cy + 1.0) / distance.max(0.001));
            }
        }
    }
    scale
}

pub fn content_key(request: &Request) -> Result<String, Error> {
    let mut hash = blake3::Hasher::new();
    hash.update(VERSION.as_bytes());
    hash.update(&serde_json::to_vec(&(
        request.motion,
        request.strength,
        request.frames,
        request.fps,
        Quality::default(),
    ))?);
    hash.update(
        &retold_render::Attachments::key(&request.attached)
            .map_err(|e| Error::Invalid(e.to_string()))?,
    );
    for path in std::iter::once(&request.image)
        .chain(std::iter::once(&request.depth))
        .chain(request.background.iter())
    {
        let bytes = std::fs::read(path)?;
        hash.update(
            &u64::try_from(bytes.len())
                .map_err(|e| Error::Invalid(e.to_string()))?
                .to_le_bytes(),
        );
        hash.update(&bytes);
    }
    Ok(hash.finalize().to_hex().to_string())
}
pub async fn render(
    request: &Request,
    cache: &Path,
    cancel: &CancellationToken,
) -> Result<Rendered, Error> {
    render_with_quality(request, cache, Quality::default(), cancel).await
}
pub async fn render_with_quality(
    request: &Request,
    cache: &Path,
    quality: Quality,
    cancel: &CancellationToken,
) -> Result<Rendered, Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if !quality.max_hole_ratio.is_finite()
        || !quality.max_stretched_fraction.is_finite()
        || !(0.0..=1.0).contains(&quality.max_hole_ratio)
        || !(0.0..=1.0).contains(&quality.max_stretched_fraction)
    {
        return Err(Error::Invalid("invalid parallax QA limits".into()));
    }
    let scene = Scene::load(request)?;
    let size = scene.dimensions();
    let key = blake3::hash(&serde_json::to_vec(&(content_key(request)?, quality))?)
        .to_hex()
        .to_string();
    std::fs::create_dir_all(cache)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.join(format!("{key}.lock")))?;
    lock.try_lock()
        .map_err(|e| Error::Invalid(format!("parallax shot already rendering: {e}")))?;
    let record = cache.join(format!("{key}.json"));
    if let Ok(bytes) = std::fs::read(&record) {
        let saved: Rendered = serde_json::from_slice(&bytes)?;
        if saved.key == key
            && std::fs::read(&saved.path)
                .is_ok_and(|b| blake3::hash(&b).to_hex().as_str() == saved.clip_hash)
        {
            return Ok(saved);
        }
    }
    remove_interrupted(cache, &key)?;
    let mut rejected = Vec::new();
    for (strength, fallback) in [
        (request.strength, Fallback::Requested),
        (request.strength * 0.5, Fallback::Gentler),
        (0.0, Fallback::Flat),
    ] {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let temporary = tempfile::Builder::new()
            .prefix(&format!("{key}.partial-"))
            .tempfile_in(cache)?;
        let audit = Arc::new(Mutex::new(Audit::default()));
        let summary = Arc::clone(&audit);
        let scene = scene.clone();
        let motion = request.motion;
        let count = request.frames;
        let frames = frame_iterator(scene, motion, count, strength, quality, summary);
        let encoded =
            retold_render::encode_frames(frames, size, request.fps, temporary.path(), cancel).await;
        let audit = *audit.lock().map_err(|e| Error::Invalid(e.to_string()))?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let hashes = match encoded {
            Ok(h) => h,
            Err(_) if !quality.passes(audit) => {
                rejected.push(audit);
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let path = cache.join(format!("{key}.mov"));
        let clip_hash = blake3::hash(&std::fs::read(temporary.path())?)
            .to_hex()
            .to_string();
        temporary.as_file().sync_all()?;
        temporary.persist(&path).map_err(|e| e.error)?;
        let result = Rendered {
            key,
            path,
            clip_hash,
            frame_hashes: hashes,
            fallback,
            applied_strength: strength,
            audit,
            rejected,
        };
        save_record(&record, &result)?;
        return Ok(result);
    }
    Err(Error::Invalid("parallax fallback exhausted".into()))
}
fn save_record(path: &Path, value: &Rendered) -> Result<(), Error> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn frame_iterator(
    scene: Scene,
    motion: CameraMove,
    count: u32,
    strength: f64,
    quality: Quality,
    summary: Arc<Mutex<Audit>>,
) -> impl Iterator<Item = Result<RgbaImage, retold_render::Error>> + Send {
    (0..count).map(move |index| {
        let progress = if count == 1 {
            0.0
        } else {
            f64::from(index) / f64::from(count - 1)
        };
        let (image, current) = scene
            .frame_at(index, motion, progress, strength)
            .map_err(|e| retold_render::Error::Invalid(e.to_string()))?;
        let mut total = summary
            .lock()
            .map_err(|e| retold_render::Error::Invalid(e.to_string()))?;
        total.hole_ratio = total.hole_ratio.max(current.hole_ratio);
        total.stretched_fraction = total.stretched_fraction.max(current.stretched_fraction);
        total.max_stretch = total.max_stretch.max(current.max_stretch);
        total.unresolved_holes = total.unresolved_holes.max(current.unresolved_holes);
        total.edge_holes = total.edge_holes.max(current.edge_holes);
        total.framing_scale = total.framing_scale.max(current.framing_scale);
        total.fill_radius_ratio = total.fill_radius_ratio.max(current.fill_radius_ratio);
        drop(total);
        if !quality.passes(current) {
            return Err(retold_render::Error::Invalid(
                "parallax QA rejected the move".into(),
            ));
        }
        Ok(image)
    })
}

fn remove_interrupted(cache: &Path, key: &str) -> Result<(), Error> {
    let prefix = format!("{key}.partial-");
    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn load_scene_from_request(request: &Request) -> Result<Scene, Error> {
    if request.frames == 0
        || request.frames > 36_000
        || request.fps == 0
        || request.fps > 120
        || !request.strength.is_finite()
        || !(0.0..=1.0).contains(&request.strength)
    {
        return Err(Error::Invalid(
            "invalid parallax frame count, fps or strength".into(),
        ));
    }
    let image = crate::load_image(&request.image)?.into_rgba8();
    let depth = DepthMap::load(&request.depth)?;
    if image.dimensions() != (depth.width, depth.height) {
        return Err(Error::Invalid("scene and depth dimensions differ".into()));
    }
    let background = request
        .background
        .as_ref()
        .map(|p| crate::load_image(p).map(image::DynamicImage::into_rgba8))
        .transpose()?;
    if background
        .as_ref()
        .is_some_and(|b| b.dimensions() != image.dimensions())
    {
        return Err(Error::Invalid("background plate dimensions differ".into()));
    }
    let attached = retold_render::Attachments::load(&request.attached, request.frames)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(Scene {
        image: Arc::new(image),
        depth: Arc::new(depth),
        background: background.map(Arc::new),
        attached: Arc::new(attached),
    })
}
