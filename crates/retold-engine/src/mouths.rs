//! S4 mouths: per-character mouth sheets, per-frame face landmarks and phoneme-timed
//! mouth and blink sprites attached to each speaking shot.
//!
//! Every step degrades to the plain still: a unit without a visible speaker, a sheet
//! that fails its checks or a composite the vision check rejects keeps its artwork.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use retold_lipsync::fit::Anchor;
use retold_lipsync::{Eyes, Viseme};
use retold_providers::media::ImageRequest;
use retold_render::Sprite;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assets::Images;
use crate::context::Context;
use crate::languages::Edition;
use crate::media_runtime::{Asset, MediaError};
use crate::store::hash;
use crate::{EngineError, fsx};

const SHEET_QA_POLICY: u32 = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Point {
    /// Fraction of the image width, 0 at the left edge.
    x: f64,
    /// Fraction of the image height, 0 at the top edge.
    y: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FaceBox {
    name: String,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Faces {
    faces: Vec<FaceBox>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Features {
    /// The mouth is visible and seen roughly from the front (not in profile or covered).
    mouth_visible: bool,
    /// Both eyes are visible and roughly frontal.
    eyes_visible: bool,
    mouth_left: Point,
    mouth_right: Point,
    left_eye: Point,
    right_eye: Point,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Check {
    ok: bool,
    problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SheetCheck {
    ok: bool,
    problems: Vec<String>,
    /// One-based cells that need a shape/identity repair; empty for a global layout defect.
    bad_cells: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SpriteAnchor {
    label: String,
    first: Point,
    second: Point,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SpriteAnchors {
    anchors: Vec<SpriteAnchor>,
}

/// A face found in a frame, in frame pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Face {
    pub name: String,
    pub mouth: Option<Anchor>,
    pub eyes: Option<Anchor>,
}

/// Sliced, keyed sprites of one character with their own feature anchors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouthSheet {
    pub sheet: Asset,
    pub mouths: BTreeMap<Viseme, PathBuf>,
    pub eyes: BTreeMap<String, PathBuf>,
    pub mouth: Anchor,
    pub eye_line: Anchor,
    #[serde(default)]
    pub anchors: BTreeMap<String, Anchor>,
    #[serde(default)]
    pub qa_policy: u32,
}

fn setup(e: impl std::fmt::Display) -> MediaError {
    EngineError::Setup(e.to_string()).into()
}

fn anchor(a: Point, b: Point, size: (u32, u32), offset: (f64, f64), scale: f64) -> Anchor {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let (ax, ay) = (a.x * w, a.y * h);
    let (bx, by) = (b.x * w, b.y * h);
    Anchor {
        x: offset.0 + f64::midpoint(ax, bx) * scale,
        y: offset.1 + f64::midpoint(ay, by) * scale,
        width: (bx - ax).hypot(by - ay) * scale,
        angle: (by - ay).atan2(bx - ax),
    }
}

const fn valid(p: Point) -> bool {
    p.x >= 0.0 && p.x <= 1.0 && p.y >= 0.0 && p.y <= 1.0
}

/// Mouth/eye features of one face image.
async fn features(
    context: &Context,
    step: &str,
    image: &Path,
    name: &str,
) -> Result<Features, MediaError> {
    let features: Features = context
        .ask(
            step,
            context.guide.manifest().roles.image_qa,
            format!(
                "Locate the facial features of {name} in this image. Give positions as fractions of the image width (x, from the left) and height (y, from the top). mouth_left and mouth_right are the two corners of the mouth as seen in the image (left = smaller x). left_eye and right_eye are the centres of the pupils (left = smaller x). Set mouth_visible false if the mouth is hidden, in profile, or too small to see; eyes_visible false likewise. Be precise: these points are used to animate the mouth and eyes."
            ),
            vec![image.to_path_buf()],
        )
        .await?;
    Ok(features)
}

fn flatten(image: &image::RgbaImage) -> image::RgbaImage {
    let mut out = image::RgbaImage::from_pixel(
        image.width(),
        image.height(),
        image::Rgba([128, 128, 128, 255]),
    );
    image::imageops::overlay(&mut out, image, 0, 0);
    out
}

/// Mouth sheet for a cast member, cached beside the cast so a season reuses it.
#[expect(
    clippy::too_many_lines,
    reason = "generate, slice, check and measure with one bounded repair"
)]
async fn sheet(
    context: &Context,
    name: &str,
    character: &Asset,
    style: &Asset,
) -> Result<Option<MouthSheet>, MediaError> {
    character.verify()?;
    style.verify()?;
    let style_prompt = crate::assets::render_style(context)?;
    let folder = context.layout.cast_dir().join("mouths");
    std::fs::create_dir_all(&folder).map_err(|e| EngineError::io(&folder, e))?;
    let _lock = crate::assets::shared_lock(&folder.join(".mouths.lock"), context).await?;
    let id = retold_core::Slug::from_title(name).map_err(setup)?;
    let revision = context
        .media
        .store
        .lock()
        .await
        .revision(&format!("mouth-sheet-{id}"));
    let key = hash(&(
        2,
        revision,
        &character.content_hash,
        &style.content_hash,
        retold_lipsync::sheet::layout_prompt(),
        &style_prompt,
    ))?;
    let record = folder.join(format!("{id}-{key}.json"));
    let mut reusable = None;
    if let Ok(bytes) = std::fs::read(&record) {
        let mut saved: MouthSheet = serde_json::from_slice(&bytes).map_err(setup)?;
        if saved.sheet.verify().is_ok()
            && saved
                .mouths
                .values()
                .chain(saved.eyes.values())
                .all(|p| p.is_file())
        {
            if saved.qa_policy == SHEET_QA_POLICY {
                if saved.anchors.len() != 10
                    || saved.mouths.keys().any(|v| {
                        !saved
                            .anchors
                            .contains_key(&format!("mouth-{v:?}").to_lowercase())
                    })
                    || ["half", "closed"]
                        .iter()
                        .any(|s| !saved.anchors.contains_key(&format!("eyes-{s}")))
                {
                    saved.anchors = sprite_anchors(context, name, &saved).await?;
                    if saved.anchors.len() != 10 {
                        return Ok(None);
                    }
                    fsx::write_json_atomic(&record, &saved)?;
                }
                return Ok(Some(saved));
            }
            reusable = Some(saved.sheet);
        }
    }
    let mut note = String::new();
    let mut previous: Option<PathBuf> = None;
    let mut bad_cells = Vec::new();
    for attempt in 0..2 {
        let step = format!("mouth-sheet-{id}-{attempt}");
        let mut request = ImageRequest {
            prompt: format!(
                "{}\n{}\nThe character is {name}, exactly as in the attached character sheet: copy their skin tone, lip colour, eye shape and line work precisely, in the attached style.{note}",
                style_prompt,
                retold_lipsync::sheet::layout_prompt(),
            ),
            references: vec![character.path.clone(), style.path.clone()],
            aspect: "4:3".into(),
            resolution: "2K".into(),
        };
        if let Some(path) = &previous {
            request.references.push(path.clone());
            request.prompt.push_str("\nThe third reference is the previous sheet. Preserve its grid, character identity and every correct cell. Repair only the listed defects; cell 7 must have the upper teeth in direct contact with the lower lip, no open gap.");
        }
        request.prompt = context.prompt(&step, &request.prompt)?;
        let asset = if attempt == 0 && reusable.is_some() {
            reusable
                .take()
                .ok_or_else(|| setup("reusable sheet missing"))?
        } else if let Some(path) = &previous
            && (1..=4).contains(&bad_cells.len())
        {
            repair_cells(context, id.as_str(), path, &bad_cells, character, style).await?
        } else {
            context
                .media
                .image(
                    &step,
                    context.guide.manifest().roles.image,
                    &request,
                    &folder.join(format!("{id}-sheet-{attempt}.png")),
                )
                .await?
        };
        previous = Some(asset.path.clone());
        let image = image::open(&asset.path).map_err(setup)?.into_rgba8();
        let sliced = match retold_lipsync::sheet::slice(&image) {
            Ok(s) => s,
            Err(e) => {
                note = format!(
                    "\nThe previous sheet was unusable: {e}. Follow the grid and magenta background exactly."
                );
                continue;
            }
        };
        let mut checks = vec![asset.path.clone(), character.path.clone()];
        for viseme in Viseme::ALL {
            let probe =
                folder.join(format!("{id}-{key}-{viseme:?}-{attempt}-qa.png").to_lowercase());
            flatten(
                sliced
                    .mouth(viseme)
                    .ok_or_else(|| setup("mouth shape missing"))?,
            )
            .save(&probe)
            .map_err(setup)?;
            checks.push(probe);
        }
        let check: SheetCheck = context
            .ask(
                &format!("mouth-sheet-check-{id}-{attempt}"),
                context.guide.manifest().roles.image_qa,
                format!("The first image is a mouth and eye sheet for {name}; the second is their character sheet. Images 3-10 are enlarged mouth cells 1-8, in that order. Judge EVERY shape from its close-up, not the small overview, and report ALL bad cells in this pass. Every open mouth must retain recognisable upper and lower lip contours in the character's original line work; a plain angular dark cut-out with no lip contour is invalid. In particular U must have a small round opening, F/V must have tooth-to-lip contact, and L must show the tongue tip against the upper teeth. A thin dark ink outline where teeth touch the lip is expected linework, not an open cavity. Reject F/V only for visible mouth interior between the lower edge of the central upper teeth and the lower lip. {} Is every cell the requested shape, the same character (lip design, eye shape, glasses and brows), in the same style, and is the layout followed? The fitting step colour-matches uniform skin-shade differences; judge colour seams in the final composite instead. Set ok false and list problems if not. Give bad_cells as one-based cell numbers (1-11) requiring shape/identity repairs, or empty for a global layout defect.", retold_lipsync::sheet::layout_prompt()),
                checks,
            )
            .await?;
        if !check.ok {
            bad_cells = check
                .bad_cells
                .into_iter()
                .filter(|c| (1..=11).contains(c))
                .collect();
            bad_cells.sort_unstable();
            bad_cells.dedup();
            note = format!(
                "\nThe previous sheet had problems: {}.",
                check.problems.join("; ")
            );
            continue;
        }
        let mut mouths = BTreeMap::new();
        for (viseme, sprite) in &sliced.mouths {
            let path = folder.join(format!("{id}-{key}-{viseme:?}.png").to_lowercase());
            sprite.save(&path).map_err(setup)?;
            mouths.insert(*viseme, path);
        }
        let mut eyes = BTreeMap::new();
        for (state, sprite) in &sliced.eyes {
            let path = folder.join(format!("{id}-{key}-eyes-{state:?}.png").to_lowercase());
            sprite.save(&path).map_err(setup)?;
            eyes.insert(format!("{state:?}").to_lowercase(), path);
        }
        // Measure the sheet's own feature positions so placement maps feature to feature.
        let rest = sliced
            .mouth(Viseme::Rest)
            .ok_or_else(|| setup("rest mouth missing"))?;
        let open = sliced
            .eyes(Eyes::Open)
            .ok_or_else(|| setup("open eyes missing"))?;
        let probe = folder.join(format!("{id}-{key}-probe-mouth.png"));
        flatten(rest).save(&probe).map_err(setup)?;
        let m = features(
            context,
            &format!("mouth-sheet-anchor-{id}-{attempt}"),
            &probe,
            name,
        )
        .await?;
        let probe_eyes = folder.join(format!("{id}-{key}-probe-eyes.png"));
        flatten(open).save(&probe_eyes).map_err(setup)?;
        let e = features(
            context,
            &format!("mouth-sheet-eyes-{id}-{attempt}"),
            &probe_eyes,
            name,
        )
        .await?;
        if !(m.mouth_visible
            && valid(m.mouth_left)
            && valid(m.mouth_right)
            && e.eyes_visible
            && valid(e.left_eye)
            && valid(e.right_eye))
        {
            note = "\nThe previous sheet's mouth or eyes could not be located; draw them clearly."
                .into();
            continue;
        }
        let mut saved = MouthSheet {
            anchors: BTreeMap::new(),
            qa_policy: SHEET_QA_POLICY,
            sheet: asset,
            mouths,
            eyes,
            mouth: anchor(
                m.mouth_left,
                m.mouth_right,
                rest.dimensions(),
                (0.0, 0.0),
                1.0,
            ),
            eye_line: anchor(e.left_eye, e.right_eye, open.dimensions(), (0.0, 0.0), 1.0),
        };
        saved.anchors = sprite_anchors(context, name, &saved).await?;
        if saved.anchors.len() != 10 {
            return Ok(None);
        }
        fsx::write_json_atomic(&record, &saved)?;
        return Ok(Some(saved));
    }
    context.media.store.lock().await.emit(
        &format!("mouths-sheet-{id}"),
        crate::events::Status::Running,
        None,
        Some(format!(
            "mouth sheet for {name} failed its checks; their shots keep still mouths"
        )),
    )?;
    Ok(None)
}

/// Each expression has its own geometry: a grid can be centred while mouth corners
/// and closed eyelids move within a cell. Measure the expression actually being fitted.
async fn sprite_anchors(
    context: &Context,
    name: &str,
    sheet: &MouthSheet,
) -> Result<BTreeMap<String, Anchor>, MediaError> {
    let mut entries: Vec<_> = sheet
        .mouths
        .iter()
        .map(|(v, p)| (format!("mouth-{v:?}").to_lowercase(), p.clone()))
        .collect();
    entries.extend(
        sheet
            .eyes
            .iter()
            .filter(|(state, _)| state.as_str() != "open")
            .map(|(state, p)| (format!("eyes-{state}"), p.clone())),
    );
    let mut references = Vec::new();
    let mut sizes = BTreeMap::new();
    for (label, path) in &entries {
        let image = image::open(path).map_err(setup)?.into_rgba8();
        let probe = path.with_file_name(format!(
            "{}-probe-{label}.png",
            retold_core::Slug::from_title(name).map_err(setup)?
        ));
        flatten(&image).save(&probe).map_err(setup)?;
        sizes.insert(label.clone(), image.dimensions());
        references.push(probe);
    }
    let labels: Vec<_> = entries.iter().map(|e| &e.0).collect();
    let measured: SpriteAnchors = context.ask(&format!("mouth-expressions-{}",retold_core::Slug::from_title(name).map_err(setup)?),context.guide.manifest().roles.image_qa,
        format!("Measure the two placement points in EACH supplied expression patch of {name}, in order. Mouth images: first and second are the actual left and right mouth CORNERS, not the centre of the patch or teeth. Eye images: first and second are the centre of the left and right eyelid opening, or centre of each CLOSED EYELID LINE for a closed blink. Ignore eyebrows and glasses. Give fractions of that individual image's width/height. Use the exact labels, each once. Labels: {}",serde_json::to_string(&labels).map_err(setup)?),references).await?;
    let mut anchors = BTreeMap::new();
    for point in measured.anchors {
        if let Some(size) = sizes.get(&point.label)
            && valid(point.first)
            && valid(point.second)
            && point.second.x - point.first.x > 0.05
            && !anchors.contains_key(&point.label)
        {
            anchors.insert(
                point.label,
                anchor(point.first, point.second, *size, (0.0, 0.0), 1.0),
            );
        } else {
            return Ok(BTreeMap::new());
        }
    }
    if anchors.len() != sizes.len() {
        anchors.clear();
    }
    Ok(anchors)
}

fn repair_instruction(cell: u32) -> &'static str {
    const INSTRUCTIONS: &[(u32, &str)] = &[
        (
            2,
            "Draw a rounded Ah opening with cream upper teeth and a small tongue. Keep a thin visible upper/lower lip contour matching the character. Never draw a plain angular dark hole.",
        ),
        (
            5,
            "The pursed lips must surround a visible SMALL ROUND opening, not a closed slit.",
        ),
        (
            7,
            "Draw a LIP BITE: the upper incisors visibly overlap a tucked lower lip. No mouth cavity between the teeth and lip, no grin or smile.",
        ),
        (
            8,
            "Raise only the tongue TIP directly against the back of the upper incisors. Hide the rest of the tongue, never put a broad tongue on the mouth floor.",
        ),
    ];
    INSTRUCTIONS
        .iter()
        .find(|(candidate_cell, ..)| *candidate_cell == cell)
        .map_or(
            "Keep the character's original lip/eye design and thin contours.",
            |(_, instruction_str)| *instruction_str,
        )
}

/// Repair isolated shape failures without asking the image model to redraw correct cells.
async fn repair_cells(
    context: &Context,
    id: &str,
    previous: &Path,
    cells: &[u32],
    character: &Asset,
    style: &Asset,
) -> Result<Asset, MediaError> {
    let mut grid = image::open(previous).map_err(setup)?.into_rgba8();
    let (width, height) = (grid.width() / 4, grid.height() / 3);
    for cell in cells {
        let index = cell - 1;
        let description = if index < 8 {
            Viseme::ALL[usize::try_from(index).map_err(setup)?]
                .description()
                .to_owned()
        } else {
            [
                "both eyes open",
                "both eyes half closed",
                "both eyes fully closed",
            ][usize::try_from(index - 8).map_err(setup)?]
            .to_owned()
        };
        let detail = repair_instruction(*cell);
        let crop = image::imageops::crop_imm(
            &grid,
            (index % 4) * width,
            (index / 4) * height,
            width,
            height,
        )
        .to_image();
        let reference = previous.with_file_name(format!("{id}-repair-reference-{cell}.png"));
        crop.save(&reference).map_err(setup)?;
        let step = format!("mouth-cell-{id}-{cell}");
        let mut request = ImageRequest {
            prompt: format!(
                "Generate ONE square animation feature patch for the character in reference 1, in the style of reference 2. Reference 3 shows the patch placement. Required expression: {description}. {detail} Repair ONLY this expression. Keep the same skin patch size and centring as reference 3, with flat pure magenta #FF00FF outside the skin oval. Draw only this mouth (or eye pair), not a face, sheet or grid. No text."
            ),
            references: vec![character.path.clone(), style.path.clone(), reference],
            aspect: "1:1".into(),
            resolution: "2K".into(),
        };
        request.prompt = context.prompt(&step, &request.prompt)?;
        let asset = context
            .media
            .image(
                &step,
                context.guide.manifest().roles.image,
                &request,
                &previous.with_file_name(format!("{id}-repair-cell-{cell}.png")),
            )
            .await?;
        let patch = image::open(&asset.path).map_err(setup)?.into_rgba8();
        let patch =
            image::imageops::resize(&patch, width, height, image::imageops::FilterType::Lanczos3);
        image::imageops::replace(
            &mut grid,
            &patch,
            i64::from((index % 4) * width),
            i64::from((index / 4) * height),
        );
    }
    let pixels = blake3::hash(grid.as_raw()).to_hex().to_string();
    let path = previous.with_file_name(format!("{id}-repaired-grid-{}.png", &pixels[..16]));
    grid.save(&path).map_err(setup)?;
    let content_hash = blake3::hash(&std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?)
        .to_hex()
        .to_string();
    Ok(Asset {
        path,
        content_hash: content_hash.clone(),
        request_id: content_hash,
        provider: "derived".into(),
        model: "mouth-cell-repair-v1".into(),
        usage: retold_providers::media::MediaUsage {
            generation: None,
            cost: Some(retold_core::Usd::ZERO),
        },
        quote: retold_providers::media::Quote {
            ceiling: retold_core::Usd::ZERO,
            provider_tag: None,
        },
    })
}

fn verify_cast(frame: &Asset, characters: &BTreeMap<String, Asset>) -> Result<(), MediaError> {
    frame.verify()?;
    for character in characters.values() {
        character.verify()?;
    }
    Ok(())
}

/// Faces of the named characters in a frame: a full-frame pass finds faces, then a
/// zoomed crop of each face places the mouth and eyes precisely.
async fn landmarks(
    context: &Context,
    unit: usize,
    frame: &Asset,
    characters: &BTreeMap<String, Asset>,
) -> Result<Vec<Face>, MediaError> {
    verify_cast(frame, characters)?;
    let names: Vec<_> = characters.keys().cloned().collect();
    let mut references = vec![frame.path.clone()];
    references.extend(characters.values().map(|a| a.path.clone()));
    let found: Faces = context
        .ask(
            &format!("mouths-faces-{unit}"),
            context.guide.manifest().roles.image_qa,
            format!(
                "The first image is a scene. The other images are character sheets for {} (in that order). For each of these characters visible in the scene with their face showing, give the face bounding box (forehead to chin, ear to ear) as fractions of the scene width and height. Use the exact character name. Omit characters that are not visible.",
                names.join(", ")
            ),
            references,
        )
        .await?;
    let image = image::open(&frame.path).map_err(setup)?.into_rgba8();
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let folder = context.layout.video_dir().join("work/mouths/crops");
    std::fs::create_dir_all(&folder).map_err(|e| EngineError::io(&folder, e))?;
    let mut faces = Vec::new();
    for face in found.faces {
        if !characters.contains_key(&face.name)
            || !(0.0..=1.0).contains(&face.left)
            || !(0.0..=1.0).contains(&face.right)
            || !(0.0..=1.0).contains(&face.top)
            || !(0.0..=1.0).contains(&face.bottom)
            || face.right <= face.left
            || face.bottom <= face.top
        {
            continue;
        }
        // Pad the box by a quarter so the whole mouth and eyes are inside the crop.
        let (pw, ph) = (
            (face.right - face.left) * 0.25,
            (face.bottom - face.top) * 0.25,
        );
        let (x0, y0) = (
            ((face.left - pw) * w).max(0.0).floor(),
            ((face.top - ph) * h).max(0.0).floor(),
        );
        let (x1, y1) = (
            ((face.right + pw) * w).min(w).ceil(),
            ((face.bottom + ph) * h).min(h).ceil(),
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to image bounds"
        )]
        let (cx, cy, cw, ch) = (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
        if cw < 16 || ch < 16 {
            continue;
        }
        let crop = image::imageops::crop_imm(&image, cx, cy, cw, ch).to_image();
        let scale = (768.0 / f64::from(cw.max(ch))).max(1.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded by 768 px"
        )]
        let zoomed = image::imageops::resize(
            &crop,
            (f64::from(cw) * scale) as u32,
            (f64::from(ch) * scale) as u32,
            image::imageops::FilterType::Lanczos3,
        );
        let id = retold_core::Slug::from_title(&face.name).map_err(setup)?;
        let path = folder.join(format!("{unit}-{id}.png"));
        zoomed.save(&path).map_err(setup)?;
        let f = features(
            context,
            &format!("mouths-features-{unit}-{id}"),
            &path,
            &face.name,
        )
        .await?;
        let size = zoomed.dimensions();
        let back = 1.0 / scale;
        let place = |a: Point, b: Point| anchor(a, b, size, (x0, y0), back);
        faces.push(Face {
            name: face.name,
            mouth: (f.mouth_visible && valid(f.mouth_left) && valid(f.mouth_right))
                .then(|| place(f.mouth_left, f.mouth_right)),
            eyes: (f.eyes_visible && valid(f.left_eye) && valid(f.right_eye))
                .then(|| place(f.left_eye, f.right_eye)),
        });
    }
    Ok(faces)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Patch {
    Mouth,
    Eyes,
}

struct FitJob<'a> {
    sprite: &'a Path,
    from: Anchor,
    to: Anchor,
    frame: &'a image::RgbaImage,
    folder: &'a Path,
    patch: Patch,
}

/// Writes a fitted sprite once (content-addressed) and returns its path and position.
fn fitted(job: &FitJob<'_>) -> Result<Option<(PathBuf, i32, i32)>, MediaError> {
    let image = image::open(job.sprite).map_err(setup)?.into_rgba8();
    let Some(mut placed) = retold_lipsync::fit::place(&image, job.from, job.to, job.frame) else {
        return Ok(None);
    };
    let local = Anchor {
        x: job.to.x - f64::from(placed.x),
        y: job.to.y - f64::from(placed.y),
        ..job.to
    };
    placed.image = match job.patch {
        Patch::Eyes => {
            retold_lipsync::fit::eye_patch(&placed.image, local, job.frame, (placed.x, placed.y))
        }
        Patch::Mouth => {
            retold_lipsync::fit::mouth_patch(&placed.image, local, job.frame, (placed.x, placed.y))
        }
    };
    if placed.image.pixels().all(|p| p[3] == 0) {
        return Ok(None);
    }
    let key = blake3::hash(placed.image.as_raw()).to_hex().to_string();
    let path = job.folder.join(format!("{}.png", &key[..32]));
    if !path.is_file() {
        let partial = path.with_extension("partial.png");
        placed.image.save(&partial).map_err(setup)?;
        std::fs::rename(&partial, &path).map_err(|e| EngineError::io(&path, e))?;
    }
    Ok(Some((path, placed.x, placed.y)))
}

/// Run-length sprites for a per-frame choice of image.
fn runs<T: PartialEq + Copy>(
    frames: &[Option<T>],
    mut sprite: impl FnMut(T) -> Option<(PathBuf, i32, i32)>,
) -> Vec<Sprite> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let mut j = i + 1;
        while j < frames.len() && frames[j] == frames[i] {
            j += 1;
        }
        if let Some(value) = frames[i]
            && let Some((image, x, y)) = sprite(value)
        {
            out.push(Sprite {
                image,
                x,
                y,
                first_frame: u32::try_from(i).unwrap_or(u32::MAX),
                end_frame: u32::try_from(j).unwrap_or(u32::MAX),
            });
        }
        i = j;
    }
    out
}

/// Mouth and blink sprites for one unit's shot, or none when lip-sync does not apply.
#[expect(
    clippy::too_many_lines,
    reason = "one ordered pass over a unit's turns, faces and checks"
)]
pub async fn attachments(
    context: &Context,
    images: &Images,
    edition: &Edition,
    index: usize,
    frames: u32,
    start: f64,
) -> Result<Vec<Sprite>, MediaError> {
    let manifest = context.guide.manifest();
    if !manifest.visuals.lipsync.enabled
        || manifest.narration.mode != retold_guide::manifest::NarrationMode::Dialogue
    {
        return Ok(Vec::new());
    }
    let Ok(script) = retold_voice::DialogueScript::parse(&edition.units[index].text) else {
        return Ok(Vec::new());
    };
    let words = &edition.alignment.units[index].words;
    if script
        .turns
        .iter()
        .map(|t| t.text.split_whitespace().count())
        .sum::<usize>()
        != words.len()
    {
        return Ok(Vec::new());
    }
    let frame = &images.frames[index];
    let on_screen: BTreeMap<String, Asset> = images
        .sheets
        .iter()
        .filter(|(name, _)| script.turns.iter().any(|t| &t.speaker == *name))
        .map(|(n, a)| (n.clone(), a.clone()))
        .collect();
    if on_screen.is_empty() {
        return Ok(Vec::new());
    }
    let faces = landmarks(context, index, &frame.image, &images.sheets).await?;
    if !faces
        .iter()
        .any(|f| f.mouth.is_some() && on_screen.contains_key(&f.name))
    {
        return Ok(Vec::new());
    }
    let artwork = image::open(&frame.image.path).map_err(setup)?.into_rgba8();
    let folder = context.layout.video_dir().join("work/mouths/sprites");
    std::fs::create_dir_all(&folder).map_err(|e| EngineError::io(&folder, e))?;
    let fps = manifest.format.fps.get();
    let language = edition.variant.narration.language().to_owned();
    let mut sheets = BTreeMap::new();
    for face in &faces {
        if let Some(character) = images.sheets.get(&face.name)
            && let Some(sheet) = sheet(context, &face.name, character, &images.style).await?
        {
            sheets.insert(face.name.clone(), sheet);
        } else if face.mouth.is_some() && on_screen.contains_key(&face.name) {
            return Ok(Vec::new());
        }
    }
    let mut sprites = Vec::new();
    let mut test_frame = artwork.clone();
    let mut consumed = 0;
    for turn in &script.turns {
        let count = turn.text.split_whitespace().count();
        let turn_words = words
            .get(consumed..(consumed + count).min(words.len()))
            .unwrap_or_default();
        consumed += count;
        let (Some(face), Some(sheet)) = (
            faces.iter().find(|f| f.name == turn.speaker),
            sheets.get(&turn.speaker),
        ) else {
            continue;
        };
        let Some(mouth) = face.mouth else { continue };
        let (Some(first), Some(last)) = (turn_words.first(), turn_words.last()) else {
            continue;
        };
        // Alignment may interpolate an unheard token to a zero-length point.
        // Keep it in turn indexing, but let the acoustic model hear it in context
        // rather than imposing an impossible zero-duration word window.
        let audible: Vec<_> = turn_words
            .iter()
            .filter(|w| w.end > w.start)
            .cloned()
            .collect();
        if audible.is_empty() {
            return Ok(Vec::new());
        }
        let spoken = context.phonemes.phonemize(&language, &audible).await?;
        let aligned = match crate::phonemes::align(context, edition, index, &spoken).await {
            Ok(aligned) => aligned,
            Err(MediaError::Quality(reason)) => {
                context.media.store.lock().await.emit(
                    &format!("mouths-{index}"),
                    crate::events::Status::Running,
                    None,
                    Some(format!(
                        "acoustic timing rejected ({reason}); keeping the still"
                    )),
                )?;
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        let speech = (
            aligned.phonemes.first().map_or(first.start, |p| p.start),
            aligned.phonemes.last().map_or(last.end, |p| p.end),
        );
        let plan = retold_lipsync::viseme::timed_plan(
            &aligned.phonemes,
            speech,
            start,
            fps,
            frames,
            &language,
        );
        fsx::write_json_atomic(
            &folder
                .parent()
                .ok_or_else(|| setup("mouth sprite folder has no parent"))?
                .join(format!("timing-{language}-{index}-{consumed}.json")),
            &serde_json::json!({"speaker":turn.speaker,"audio_hash":edition.audio.content_hash,"shot_start":start,"speech":speech,"fps":fps,
                "words":spoken,"phonemes":aligned.phonemes,"frames":plan}),
        )?;
        let mut placed = BTreeMap::new();
        for viseme in Viseme::ALL {
            let job = FitJob {
                sprite: &sheet.mouths[&viseme],
                from: sheet.anchors[&format!("mouth-{viseme:?}").to_lowercase()],
                to: mouth,
                frame: &artwork,
                folder: &folder,
                patch: Patch::Mouth,
            };
            if let Some(fit) = fitted(&job)? {
                placed.insert(viseme, fit);
            }
        }
        if placed.len() != Viseme::ALL.len() {
            return Ok(Vec::new());
        }
        if let Some((path, x, y)) = placed.get(&Viseme::Ai) {
            let sprite = image::open(path).map_err(setup)?.into_rgba8();
            image::imageops::overlay(&mut test_frame, &sprite, i64::from(*x), i64::from(*y));
        }
        sprites.extend(runs(&plan, |v| placed.get(&v).cloned()));
    }
    // Everyone visible blinks, on their own seeded rhythm.
    for face in &faces {
        let (Some(eyes), Some(sheet)) = (face.eyes, sheets.get(&face.name)) else {
            continue;
        };
        let mut placed = BTreeMap::new();
        for state in [Eyes::Half, Eyes::Closed] {
            let key = format!("{state:?}").to_lowercase();
            if let Some(path) = sheet.eyes.get(&key) {
                let job = FitJob {
                    sprite: path,
                    from: sheet.anchors[&format!("eyes-{key}")],
                    to: eyes,
                    frame: &artwork,
                    folder: &folder,
                    patch: Patch::Eyes,
                };
                if let Some(fit) = fitted(&job)? {
                    placed.insert(key, fit);
                }
            }
        }
        let seed = u64::from_le_bytes(
            blake3::hash(format!("{index}:{}", face.name).as_bytes()).as_bytes()[..8]
                .try_into()
                .map_err(setup)?,
        );
        let plan: Vec<Option<Eyes>> = retold_lipsync::viseme::blinks(frames, fps, seed)
            .into_iter()
            .map(|e| (e != Eyes::Open).then_some(e))
            .collect();
        if let Some((path, x, y)) = placed.get("closed") {
            let sprite = image::open(path).map_err(setup)?.into_rgba8();
            image::imageops::overlay(&mut test_frame, &sprite, i64::from(*x), i64::from(*y));
        }
        sprites.extend(runs(&plan, |e| {
            placed.get(&format!("{e:?}").to_lowercase()).cloned()
        }));
    }
    if sprites.is_empty() {
        return Ok(sprites);
    }
    // One composite with open mouths and closed eyes must look drawn, not pasted.
    let check_path = context.layout.video_dir().join(format!(
        "work/mouths/check-{index}-{}.png",
        &blake3::hash(test_frame.as_raw()).to_hex()[..16]
    ));
    test_frame.save(&check_path).map_err(setup)?;
    let check: Check = context
        .ask(
            &format!("mouths-check-{index}"),
            manifest.roles.image_qa,
            "The first image is the original artwork; in the second, the speaking characters' mouths are open and everyone's eyes are closed by pasted patches. Do the patches sit exactly on the faces (mouth on the mouth, eyes on the eyes), at the right size and angle, with matching skin colour, line work and style, so the result looks hand-drawn? Set ok false and list problems if any patch is misplaced, mis-sized or visibly pasted.".into(),
            vec![frame.image.path.clone(), check_path.clone()],
        )
        .await?;
    fsx::write_json_atomic(
        &check_path.with_extension("audit.json"),
        &serde_json::json!({"source":frame.image,"composite":check_path,"sprites":sprites,"qa":check}),
    )?;
    if !check.ok {
        context.media.store.lock().await.emit(
            &format!("mouths-{index}"),
            crate::events::Status::Running,
            None,
            Some(format!(
                "lip-sync for unit {} rejected ({}); keeping the still",
                index + 1,
                check.problems.join("; ")
            )),
        )?;
        return Ok(Vec::new());
    }
    Ok(sprites)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_runs_preserve_silence_and_exclusive_frame_boundaries() {
        let frames = [
            None,
            Some(Viseme::Mbp),
            Some(Viseme::Mbp),
            None,
            Some(Viseme::Ai),
            Some(Viseme::Ai),
            Some(Viseme::Rest),
        ];
        let sprites = runs(&frames, |v| {
            Some((PathBuf::from(format!("{v:?}.png")), 12, 34))
        });
        assert_eq!(sprites.len(), 3);
        assert_eq!(
            sprites
                .iter()
                .map(|s| (s.first_frame, s.end_frame))
                .collect::<Vec<_>>(),
            [(1, 3), (4, 6), (6, 7)]
        );
        assert!(sprites.iter().all(|s| s.x == 12 && s.y == 34));
        assert!(runs::<Viseme>(&[], |_| unreachable!()).is_empty());
        assert!(runs(&frames, |_| None).is_empty());
    }

    #[test]
    fn crop_landmarks_map_back_to_original_frame() {
        let placed = anchor(
            Point { x: 0.25, y: 0.5 },
            Point { x: 0.75, y: 0.5 },
            (768, 384),
            (100.0, 200.0),
            0.25,
        );
        assert!((placed.x - 196.0).abs() < 1e-9);
        assert!((placed.y - 248.0).abs() < 1e-9);
        assert!((placed.width - 96.0).abs() < 1e-9);
        assert!(placed.angle.abs() < 1e-9);
        assert!(!valid(Point {
            x: f64::NAN,
            y: 0.5
        }));
        assert!(!valid(Point { x: 0.5, y: 1.01 }));
    }
}
