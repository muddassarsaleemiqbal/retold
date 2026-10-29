//! Character designs, shared sheets, style reference, covers and per-unit image QA.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use retold_core::Slug;
use retold_guide::template;
use retold_providers::media::ImageRequest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinSet;

use crate::context::Context;
use crate::media_runtime::{Asset, MediaError};
use crate::store::hash;
use crate::story::{Direction, Plan, Unit};
use crate::{EngineError, Stage, fsx};
use retold_core::num;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storyboard {
    pub plan: Plan,
    pub units: Vec<Unit>,
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub name: String,
    pub appearance: String,
    pub clothes: String,
    pub palette: Vec<String>,
    pub distinguishing_detail: String,
    /// Facial expressions the story needs; the reference sheet shows each one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<String>,
    /// Apparent age, e.g. "early seventies". The fields below lock the design so every
    /// frame redraws the same person; cards saved before they existed leave them empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub age: String,
    /// Height in heads, frame and resting posture, e.g. "7 heads tall, thin, slight hunch".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub build: String,
    /// Head shape, skin, eyes, brows, nose and default mouth.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub face: String,
    /// Hairline, length, cut and colour (or baldness), and facial hair.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hair: String,
    /// One exact colour per visible part (skin, hair, each garment, shoes, prop).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<Swatch>,
    /// One pose per expression, in the same order, as the sheet draws them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub poses: Vec<Pose>,
    /// Speaking voice for casting: "female"/"male" (or another gender), age and timbre.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub voice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Swatch {
    /// e.g. "skin", "hair", "suit", "tie", "shoes", "cane".
    pub part: String,
    /// "#RRGGBB".
    pub hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    /// The expression this pose shows, exactly as listed in `expressions`.
    pub expression: String,
    /// Brows, eyes and mouth for this expression.
    pub face: String,
    /// Stance, arms, what each hand does, and any prop held.
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cast {
    pub characters: Vec<Character>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageQa {
    pub style_score: f64,
    pub character_score: f64,
    pub text_present: bool,
    pub anatomy_errors: Vec<String>,
    /// Defects that make the frame unusable; any entry fails the frame.
    pub problems: Vec<String>,
    /// Smaller deviations shown in the review page; they do not fail the frame. Omitted when
    /// empty so check results saved without this field keep their exact form (they
    /// are part of repair prompts, and so of cache keys).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub focus: [f64; 2],
}

impl ImageQa {
    #[must_use]
    pub fn passes(&self) -> bool {
        [self.style_score, self.character_score]
            .iter()
            .all(|s| s.is_finite() && (7.0..=10.0).contains(s))
            && !self.text_present
            && self.anatomy_errors.is_empty()
            && self.problems.is_empty()
            && self
                .focus
                .iter()
                .all(|f| f.is_finite() && (0.0..=1.0).contains(f))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub unit: usize,
    pub image: Asset,
    pub qa: ImageQa,
    pub regenerated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Images {
    pub cover: Asset,
    pub frames: Vec<Frame>,
    pub style: Asset,
    pub sheets: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameBudget {
    Standard,
    Economy,
}

/// Worst-case image price when every image falls back to a paid provider.
const PAID_IMAGE_PRICE: retold_core::Usd = retold_core::Usd::from_micros(19_000);

/// Before any image is drawn: when the remaining budget could not cover the images should
/// every one fall back to a paid provider, switch to economy (frames at 1K, no automatic
/// redraws; failed frames go to review) and say so. Subscriptions usually draw them for
/// free, and the ledger still refuses any call over budget.
async fn economy_mode(context: &Context, board: &Storyboard) -> Result<FrameBudget, MediaError> {
    if !context.media.allow_paid {
        return Ok(FrameBudget::Standard);
    }
    let cached = crate::estimate::already_drawn(&context.layout.cast_dir(), board);
    let estimate = crate::estimate::images(board, &cached, PAID_IMAGE_PRICE, 0.25);
    let left = context
        .meta
        .budget
        .saturating_sub(context.media.ledger.total()?);
    if estimate.total <= left {
        return Ok(FrameBudget::Standard);
    }
    context.media.store.lock().await.emit(
        "images",
        crate::events::Status::Running,
        None,
        Some(format!(
            "economy mode: {} images (+{} expected redraws) could cost up to {} if subscriptions are unavailable, but only {} of budget is left; frames are drawn at 1K without automatic redraws",
            estimate.images, estimate.redraws, estimate.total, left
        )),
    )?;
    Ok(FrameBudget::Economy)
}

/// Shared cast/style ownership is separate from the run lock. The stable lock file is
/// never deleted; interrupted owners release their OS lock automatically.
pub async fn shared_lock(path: &Path, context: &Context) -> Result<File, MediaError> {
    let parent = path
        .parent()
        .ok_or_else(|| EngineError::Setup("shared lock has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(|e| EngineError::io(parent, e))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| EngineError::io(path, e))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    loop {
        context.check_cancel()?;
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if tokio::time::Instant::now() < deadline => {
                tokio::select! { () = context.media.cancel.cancelled() => context.check_cancel()?, () = tokio::time::sleep(Duration::from_millis(200)) => {} }
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(EngineError::Locked {
                    path: path.into(),
                    holder: None,
                }
                .into());
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(EngineError::io(path, e).into()),
        }
    }
}

pub async fn images(context: Arc<Context>, board: Arc<Storyboard>) -> Result<Images, MediaError> {
    if context.guide.manifest().visuals.characters.review && !context.skip_reviews {
        let (_, sheets) = characters(&context, &board).await?;
        if !crate::character_approval::approved(&context.layout, &sheets)? {
            crate::review::characters(&context, &sheets)?;
            return Err(MediaError::Review(format!(
                "approve the character designs with `retold video approve {} characters`, then plan and continue again",
                crate::review::reference(&context)
            )));
        }
    }
    let economy = economy_mode(&context, &board).await?;
    let style = style(&context).await?;
    let ready = Arc::new(tokio::sync::Mutex::new(SheetProgress::default()));
    let sheet_future = async {
        let result = sheets(&context, &board, &style, Some(&ready)).await;
        let mut status = ready.lock().await;
        status.finished = true;
        status.error = result.as_ref().err().map(ToString::to_string);
        drop(status);
        let sheets = result?;
        context
            .media
            .store
            .lock()
            .await
            .commit("characters", &hash(&sheets)?, &sheets, None)?;
        let cover = cover(&context, &board, &style, &sheets).await?;
        Ok::<_, MediaError>((sheets, cover))
    };
    let places = scene_references(&context, &board, &style).await?;
    let places = Arc::new(places);
    let frame_future = frames(
        Arc::clone(&context),
        Arc::clone(&board),
        style.clone(),
        Arc::clone(&ready),
        Arc::clone(&places),
        economy,
    );
    let sheet_future = crate::events::observe_branch(sheet_future, async || {
        context.branch_failed("characters", None).await
    });
    let frame_future = crate::events::observe_branch(frame_future, async || {
        context.branch_failed("images", None).await
    });
    let (sheet_result, frames) = tokio::join!(sheet_future, frame_future);
    let (sheets, cover) = sheet_result?;
    let frames = frames?;
    let images = Images {
        cover,
        frames,
        style,
        sheets,
    };
    fsx::write_json_atomic(
        &context.layout.stage(Stage::Images).join("images.json"),
        &images,
    )?;
    let failed: Vec<_> = images
        .frames
        .iter()
        .filter(|frame| !frame.qa.passes())
        .map(|frame| frame.unit + 1)
        .collect();
    if !failed.is_empty() {
        crate::review::images_only(&context, &board, &images)?;
        return Err(MediaError::Quality(format!(
            "frames {failed:?} failed image QA after regeneration; inspect review.html, then use retold redo RUN images with these one-based numbers"
        )));
    }
    let key = hash(&images)?;
    context
        .media
        .store
        .lock()
        .await
        .commit("images", &key, &images, None)?;
    Ok(images)
}

async fn scene_references(
    context: &Context,
    board: &Storyboard,
    style: &Asset,
) -> Result<BTreeMap<String, Asset>, MediaError> {
    let shots = &board.direction.shots;
    let used_places: Vec<_> = board
        .plan
        .settings
        .iter()
        .filter(|p| shots.iter().any(|s| s.setting == p.name))
        .map(|p| (p.name.clone(), p.description.clone()))
        .collect();
    let used_props: Vec<_> = board
        .plan
        .props
        .iter()
        .filter(|p| shots.iter().any(|s| s.props.contains(p)))
        .map(|p| (p.clone(), p.clone()))
        .collect();
    let mut places = locked_refs(context, Locked::Place, &used_places, style).await?;
    for (name, asset) in locked_refs(context, Locked::Prop, &used_props, style).await? {
        places.insert(format!("prop:{name}"), asset);
    }
    Ok(places)
}

async fn cover(
    context: &Context,
    board: &Storyboard,
    style: &Asset,
    sheets: &BTreeMap<String, Asset>,
) -> Result<Asset, MediaError> {
    // Lead with the protagonist, then the planner's order (its order of importance).
    let mut leads: Vec<&String> = board.plan.characters.iter().collect();
    leads.sort_by_key(|name| {
        !board
            .plan
            .cast
            .iter()
            .any(|c| &&c.name == name && c.role.to_lowercase().contains("protagonist"))
    });
    let leads: Vec<_> = leads
        .into_iter()
        .filter(|n| sheets.contains_key(*n))
        .take(3)
        .collect();
    let mut prompt = format!(
        "{}\nA striking cover composition for {}, the kind of image that makes a viewer stop scrolling. Storyline: {}.",
        render_style(context)?,
        context.meta.title,
        board.plan.storyline
    );
    if let Some(lead) = leads.first() {
        let _ = write!(
            prompt,
            " Feature {lead} (the first character sheet attached) large and close, with a strong readable expression and a clear silhouette"
        );
        if leads.len() > 1 {
            let others: Vec<_> = leads[1..].iter().map(|s| s.as_str()).collect();
            let _ = write!(
                prompt,
                "; {} may appear smaller, behind or beside them",
                others.join(" and ")
            );
        }
        prompt.push('.');
    }
    if let Some(image) = board.plan.iconic_images.first() {
        let _ = write!(
            prompt,
            " Build the image around this iconic element of the story, clearly visible: {image}."
        );
    }
    prompt.push_str(" One focal point, bold contrast between subject and background, dramatic lighting that suits the tone. Match each character's attached sheet. No text or letters; leave the top fifth clear for a title.");
    let mut cover_request = ImageRequest {
        prompt,
        references: std::iter::once(style.path.clone())
            .chain(leads.iter().map(|n| sheets[*n].path.clone()))
            .collect(),
        aspect: context.guide.manifest().format.aspect.clone(),
        resolution: "2K".into(),
    };
    cover_request.prompt = context.prompt("cover", &cover_request.prompt)?;
    let cover_path = context.layout.stage(Stage::Images).join("cover.png");
    let cover_future = context.media.image(
        "cover",
        context.guide.manifest().roles.image,
        &cover_request,
        &cover_path,
    );
    cover_future.await
}

pub async fn characters(
    context: &Context,
    board: &Storyboard,
) -> Result<(Asset, BTreeMap<String, Asset>), MediaError> {
    let style = style(context).await?;
    let sheets = sheets(context, board, &style, None).await?;
    fsx::write_json_atomic(
        &context.layout.stage(Stage::Characters).join("review.json"),
        &sheets,
    )?;
    context
        .media
        .store
        .lock()
        .await
        .commit("characters", &hash(&sheets)?, &sheets, None)?;
    Ok((style, sheets))
}

pub(crate) fn render_style(context: &Context) -> Result<String, MediaError> {
    template::render("visuals.style",&context.guide.manifest().visuals.style,
        json!({"inputs":context.meta.inputs.template_context(),"channel":context.channel.template_context()}))
        .map_err(|e| EngineError::Template(e.to_string()).into())
}

async fn style(context: &Context) -> Result<Asset, MediaError> {
    let root = context
        .channel
        .library_path(&context.channel.file().library.style_reference);
    let _lock = shared_lock(&root.with_extension("lock"), context).await?;
    let style = render_style(context)?;
    let key = hash(&(
        1,
        context.media.store.lock().await.revision("style-reference"),
        &style,
        &context.guide.manifest().format.aspect,
    ))?;
    let cache = root.with_file_name(format!("style-{key}.json"));
    if let Some(asset) = read_shared(&cache)? {
        return Ok(asset);
    }
    let mut request = ImageRequest {
        prompt: format!(
            "{style}\nStyle reference panel for a channel: an empty, atmospheric location (a modest room with a window, furniture and a few props) with warm cream paper grain, flat muted colors and strong graphic shadows. No people, characters, faces, figures or animals: this image is copied for rendering only, so anyone in it would leak into every video. No text, borders or labels."
        ),
        references: vec![],
        aspect: context.guide.manifest().format.aspect.clone(),
        resolution: "2K".into(),
    };
    request.prompt = context.prompt("style-reference", &request.prompt)?;
    let asset = context
        .media
        .image(
            "style-reference",
            context.guide.manifest().roles.image,
            &request,
            &root,
        )
        .await?;
    fsx::write_json_atomic(&cache, &asset)?;
    Ok(asset)
}

fn read_shared(path: &Path) -> Result<Option<Asset>, MediaError> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let asset: Asset = serde_json::from_slice(&bytes).map_err(|e| EngineError::Config {
                path: path.into(),
                message: e.to_string(),
            })?;
            asset.verify()?;
            Ok(Some(asset))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EngineError::io(path, e).into()),
    }
}

/// Asks the design role for character cards the cast file lacks (or all, after a redo).
/// Why a set of character cards cannot seed the art; empty when they can.
fn card_problems(cast: &Cast, requested: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    let names: std::collections::BTreeSet<_> = cast.characters.iter().map(|c| &c.name).collect();
    if names != requested.iter().collect() || names.len() != cast.characters.len() {
        problems.push(format!(
            "give exactly one card for each of {}",
            requested.join(", ")
        ));
    }
    for c in &cast.characters {
        let appearance = c.appearance.trim().to_lowercase();
        if c.clothes.trim().is_empty() || c.palette.is_empty() {
            problems.push(format!("{}: clothes and palette must not be empty", c.name));
        }
        // An appearance must describe a body and face, not repeat the name.
        if appearance.split_whitespace().count() < 3 || appearance == c.name.trim().to_lowercase() {
            problems.push(format!("{}: appearance must describe face, hair and build in a few words, not repeat the name", c.name));
        }
    }
    for c in &cast.characters {
        problems.extend(design_problems(c));
    }
    let palette = |c: &Character| {
        let mut p: Vec<_> = if c.colors.is_empty() {
            c.palette.iter().map(|x| x.trim().to_lowercase()).collect()
        } else {
            c.colors
                .iter()
                .map(|x| x.hex.trim().to_lowercase())
                .collect()
        };
        p.sort();
        p
    };
    if cast.characters.len() > 1
        && cast
            .characters
            .iter()
            .all(|c| palette(c) == palette(&cast.characters[0]))
    {
        problems.push("every character has the same palette; give each their own colours".into());
    }
    problems
}

fn is_hex(color: &str) -> bool {
    let c = color.trim();
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

/// Why one card is too vague to redraw the same person in every frame.
fn design_problems(c: &Character) -> Vec<String> {
    let mut problems = Vec::new();
    let n = &c.name;
    for (field, value) in [
        ("age", &c.age),
        ("build", &c.build),
        ("face", &c.face),
        ("hair", &c.hair),
    ] {
        if value.split_whitespace().count() < 2 {
            problems.push(format!("{n}: {field} must be a concrete description"));
        }
    }
    if !c.build.chars().any(|x| x.is_ascii_digit()) {
        problems.push(format!(
            "{n}: build must give the height in heads, e.g. \"7.5 heads tall\""
        ));
    }
    if c.colors.len() < 3
        || c.colors
            .iter()
            .any(|s| s.part.trim().is_empty() || !is_hex(&s.hex))
    {
        problems.push(format!(
            "{n}: colors needs at least 3 parts, each with a #RRGGBB hex"
        ));
    }
    let wanted: Vec<_> = c
        .expressions
        .iter()
        .take(5)
        .map(|e| e.trim().to_lowercase())
        .collect();
    let posed: Vec<_> = c
        .poses
        .iter()
        .map(|p| p.expression.trim().to_lowercase())
        .collect();
    if wanted != posed {
        problems.push(format!(
            "{n}: give exactly one pose per expression, in order: {}",
            wanted.join(", ")
        ));
    }
    for p in &c.poses {
        if p.face.split_whitespace().count() < 3 || !p.body.to_lowercase().contains("hand") {
            problems.push(format!(
                "{n}: pose {} must describe brows, eyes and mouth, and what both hands do",
                p.expression
            ));
        }
    }
    problems
}

/// The image prompt for one reference sheet: the locked design, then one line per pose.
fn sheet_prompt(style: &str, card: &Character) -> String {
    let mut design = format!(
        "CHARACTER: {} — original design, never an actor's likeness or an iconic costume.\nLOCKED DESIGN (identical in every pose):\n",
        card.name
    );
    for (label, value) in [
        ("Age", &card.age),
        ("Build and posture", &card.build),
        ("Face", &card.face),
        ("Hair", &card.hair),
        ("Appearance", &card.appearance),
        ("Clothes", &card.clothes),
        ("Distinguishing detail", &card.distinguishing_detail),
    ] {
        if !value.trim().is_empty() {
            let _ = writeln!(design, "- {label}: {}", value.trim());
        }
    }
    if card.colors.is_empty() {
        let _ = writeln!(design, "- Palette: {}", card.palette.join(", "));
    } else {
        let colors: Vec<_> = card
            .colors
            .iter()
            .map(|s| format!("{} {}", s.part.trim(), s.hex.trim()))
            .collect();
        let _ = writeln!(
            design,
            "- Exact colours (flat, same in every pose): {}",
            colors.join(", ")
        );
    }
    let poses: Vec<(String, String)> = if card.poses.is_empty() {
        card.expressions
            .iter()
            .take(5)
            .map(|e| (e.clone(), String::new()))
            .collect()
    } else {
        card.poses
            .iter()
            .take(5)
            .map(|p| {
                (
                    p.expression.clone(),
                    format!("{}; {}", p.face.trim(), p.body.trim()),
                )
            })
            .collect()
    };
    let layout = if poses.is_empty() {
        "SHEET: one full-body figure, neutral stance, calm expression, clear face, hands visible."
            .to_owned()
    } else {
        let mut lines = format!(
            "SHEET: the same character drawn exactly {} times side by side, centered with comfortable margins on the left and right borders so no figure touches the edge, left to right, full body, equal scale, feet on one shared baseline, nothing cropped. Only the expression, arms and hands change between poses:\n",
            poses.len()
        );
        for (i, (expression, detail)) in poses.iter().enumerate() {
            let _ = writeln!(
                lines,
                "{}. {}{}",
                i + 1,
                expression.trim().to_uppercase(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {detail}")
                }
            );
        }
        lines.push_str("Brows, eyes and mouth must clearly differ between poses.");
        lines
    };
    format!(
        "{style}\n{design}{layout}\nPlain white background, no environment; a small flat light-grey oval contact shadow under the feet. Exactly two arms and two hands per figure, consistent finger count. Keep face shape, hairline, head size, body length, outfit and colours identical across poses. No text, labels, numbers or watermark. No gradients, 3D or photorealism. Match the attached style reference's line work and rendering precisely."
    )
}

/// A vision check of a reference sheet before any frame copies it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SheetQa {
    figures: usize,
    same_character: bool,
    text_present: bool,
    anatomy_errors: Vec<String>,
    /// Defects that make the sheet unusable as a reference.
    problems: Vec<String>,
}

impl SheetQa {
    const fn passes(&self, figures: usize) -> bool {
        self.figures == figures
            && self.same_character
            && !self.text_present
            && self.anatomy_errors.is_empty()
            && self.problems.is_empty()
    }
}

async fn design_missing_cards(
    context: &Context,
    board: &Storyboard,
    existing: &mut BTreeMap<String, Character>,
    cards_path: &Path,
) -> Result<(), MediaError> {
    let force_cards = context.media.store.lock().await.revision("design-cards") > 0;
    let missing: Vec<_> = board
        .plan
        .characters
        .iter()
        .filter(|name| !existing.contains_key(*name) || force_cards)
        .cloned()
        .collect();
    if !missing.is_empty() {
        let prompt = format!(
            "Design original illustrated characters for these names exactly once: {}. Story roles/context: {}. Use fresh silhouettes, ordinary clothes and original colors. Never mention actors, imitate their faces or use an iconic costume/hairstyle/prop. Lock each design so an illustrator redraws the identical person in every frame: age; build with height in heads (e.g. \"7.5 heads tall\"), frame and resting posture; face (head shape, skin, eyes, brows, nose, default mouth); hair (hairline, cut, colour, facial hair); appearance as a one-line summary; clothes garment by garment, cut to fit the story's own time, place and world (modern, historical or invented); palette; colors with one #RRGGBB hex per visible part (skin, hair, each garment, shoes, signature prop), distinct from every other character; one distinguishing detail visible at a distance; and voice: their gender (female, male or other), apparent age and timbre, e.g. \"male, elderly, dry and measured\", used to cast a matching voice. Give each character a silhouette that differs from the others (height, build, hair shape). When the context profiles a character (cast), follow its look and list its emotions as expressions (at most 5); otherwise leave expressions empty. poses: one per expression, in the same order, naming the brows, eyes and mouth (face) and the stance, arms and what each hand does, plus any prop held (body); the first pose is their default and should hide nothing the story reveals later.",
            serde_json::to_string(&missing).map_err(|e| EngineError::Setup(e.to_string()))?,
            serde_json::to_string(&board.plan).map_err(|e| EngineError::Setup(e.to_string()))?
        );
        let mut cast: Cast = context
            .ask(
                "design-cards",
                context.guide.manifest().roles.design,
                prompt.clone(),
                vec![],
            )
            .await?;
        let mut problems = card_problems(&cast, &missing);
        if !problems.is_empty() {
            // One bounded repair that names what was wrong (small local models need it).
            cast = context.ask("design-cards-repair",context.guide.manifest().roles.design,
                format!("{prompt}\nYour previous answer was rejected: {}. Answer again, fixing every problem.",problems.join("; ")),vec![]).await?;
            problems = card_problems(&cast, &missing);
        }
        if !problems.is_empty() {
            return Err(MediaError::Quality(format!(
                "character cards do not cover exactly the requested cast: {}",
                problems.join("; ")
            )));
        }
        for card in cast.characters {
            existing.insert(card.name.clone(), card);
        }
        fsx::write_json_atomic(cards_path, existing)?;
    }
    Ok(())
}

async fn sheets(
    context: &Context,
    board: &Storyboard,
    style: &Asset,
    ready: Option<&Arc<tokio::sync::Mutex<SheetProgress>>>,
) -> Result<BTreeMap<String, Asset>, MediaError> {
    let folder = context.layout.cast_dir();
    let _lock = shared_lock(&folder.join(".cast.lock"), context).await?;
    let cards_path = folder.join("characters.json");
    let mut existing: BTreeMap<String, Character> = match std::fs::read(&cards_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| EngineError::Config {
            path: cards_path.clone(),
            message: e.to_string(),
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(EngineError::io(&cards_path, e).into()),
    };
    design_missing_cards(context, board, &mut existing, &cards_path).await?;
    let looks = part_looks(context)?;
    let mut result = BTreeMap::new();
    for name in &board.plan.characters {
        let card = &existing[name];
        let id = Slug::from_title(name).map_err(|e| EngineError::Setup(e.to_string()))?;
        let key = hash(&(
            2,
            context
                .media
                .store
                .lock()
                .await
                .revision(&format!("sheet-{id}")),
            card,
            &style.content_hash,
        ))?;
        let cached = folder.join(format!("{id}-{key}.json"));
        let base = if let Some(asset) = read_shared(&cached)? {
            asset
        } else {
            let asset = draw_sheet(context, card, &id, style, &folder, None).await?;
            fsx::write_json_atomic(&cached, &asset)?;
            asset
        };
        let asset = match looks
            .as_ref()
            .and_then(|(part, by_name)| by_name.get(name).map(|l| (part, l)))
        {
            Some((part, look)) => {
                part_sheet(context, card, &base, part, look, style, &folder).await?
            }
            None => base,
        };
        if let Some(ready) = ready {
            ready
                .lock()
                .await
                .assets
                .insert(name.clone(), asset.clone());
        }
        result.insert(name.clone(), asset);
    }
    Ok(result)
}

/// A canon part id, and each character's look in it by name.
type PartLooks = (String, BTreeMap<String, String>);

/// For a canon part video: the part id and, by character name (and alias), how their look
/// differs in this part. `None` outside a canon.
fn part_looks(context: &Context) -> Result<Option<PartLooks>, MediaError> {
    let Ok(bytes) = std::fs::read(
        context
            .layout
            .stage(Stage::Research)
            .join("canon-slice.json"),
    ) else {
        return Ok(None);
    };
    let slice: crate::canon::Slice =
        serde_json::from_slice(&bytes).map_err(|e| EngineError::Setup(e.to_string()))?;
    let Some(canon) = crate::canon::load(&context.layout)? else {
        return Ok(None);
    };
    let mut looks = BTreeMap::new();
    for c in &canon.characters {
        if let Some(state) = c
            .timeline
            .iter()
            .find(|s| s.part == slice.part.id && !s.look.trim().is_empty())
        {
            for name in std::iter::once(&c.name).chain(&c.aliases) {
                looks.insert(name.clone(), state.look.trim().to_owned());
            }
        }
    }
    Ok(Some((slice.part.id, looks)))
}

/// Every card as frames of this video should show them: base cards, with this canon
/// part's look cards in their place.
fn frame_cards(context: &Context) -> Result<BTreeMap<String, Character>, MediaError> {
    let folder = context.layout.cast_dir();
    let mut cards = read_cards(&folder.join("characters.json"))?;
    if let Some((part, _)) = part_looks(context)? {
        for (key, card) in read_cards(&folder.join("looks.json"))? {
            if let Some(name) = key.strip_suffix(&format!("@{part}")) {
                cards.insert(name.to_owned(), card);
            }
        }
    }
    Ok(cards)
}

/// The same person in this part's look: a look card restyled from the base card (face,
/// build, voice, expressions and poses kept), drawn with the base sheet as reference.
async fn part_sheet(
    context: &Context,
    base: &Character,
    base_sheet: &Asset,
    part: &str,
    look: &str,
    style: &Asset,
    folder: &Path,
) -> Result<Asset, MediaError> {
    let looks_path = folder.join("looks.json");
    let mut looks = read_cards(&looks_path)?;
    let key = format!("{}@{part}", base.name);
    let id = Slug::from_title(&format!("{} {part}", base.name))
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let card = if let Some(card) = looks.get(&key) {
        card.clone()
    } else {
        let prompt = format!(
            "Restyle this character for part {part} of the story, where: {look}. Keep the same person: change clothes, colors (one #RRGGBB per visible part), palette, appearance summary, distinguishing detail, and age or hair only if the change says so. Return the whole card.\nCARD: {}",
            serde_json::to_string(base).map_err(|e| EngineError::Setup(e.to_string()))?
        );
        let mut card: Character = context
            .ask(
                &format!("design-look-{id}"),
                context.guide.manifest().roles.design,
                prompt,
                vec![],
            )
            .await?;
        // Identity never changes between parts.
        card.name.clone_from(&base.name);
        card.face.clone_from(&base.face);
        card.build.clone_from(&base.build);
        card.voice.clone_from(&base.voice);
        card.expressions.clone_from(&base.expressions);
        card.poses.clone_from(&base.poses);
        if !design_problems(&card).is_empty() {
            card = base.clone();
            card.clothes = format!("{} — in this part: {look}", base.clothes);
        }
        looks.insert(key, card.clone());
        fsx::write_json_atomic(&looks_path, &looks)?;
        card
    };
    let cache_key = hash(&(
        1,
        context
            .media
            .store
            .lock()
            .await
            .revision(&format!("sheet-{id}")),
        &card,
        &base_sheet.content_hash,
        &style.content_hash,
    ))?;
    let cached = folder.join(format!("{id}-{cache_key}.json"));
    if let Some(asset) = read_shared(&cached)? {
        return Ok(asset);
    }
    let asset = draw_sheet(context, &card, &id, style, folder, Some(&base_sheet.path)).await?;
    fsx::write_json_atomic(&cached, &asset)?;
    Ok(asset)
}

/// The image request for a sheet; with `base`, the same person's base sheet is attached.
fn sheet_request(
    context: &Context,
    card: &Character,
    id: &Slug,
    style: &Asset,
    base: Option<&Path>,
) -> Result<ImageRequest, MediaError> {
    let mut request = ImageRequest {
        prompt: match base {
            Some(_) => format!(
                "{}\nThe second attached image is this same person's base character sheet: keep the identical face, head shape, build, skin tone and proportions; change only what the design above says for this part. Its clothes belong to another part: never copy them. Every figure wears only the clothes described above.",
                sheet_prompt(&render_style(context)?, card)
            ),
            None => sheet_prompt(&render_style(context)?, card),
        },
        references: std::iter::once(style.path.clone())
            .chain(base.map(Path::to_path_buf))
            .collect(),
        aspect: context.guide.manifest().format.aspect.clone(),
        resolution: "2K".into(),
    };
    request.prompt = context.prompt(&format!("sheet-{id}"), &request.prompt)?;
    Ok(request)
}

/// Draws one reference sheet and, when the guide checks images, verifies it (one redraw).
/// `base` is the same person's base sheet, when drawing them in another part's look.
async fn draw_sheet(
    context: &Context,
    card: &Character,
    id: &Slug,
    style: &Asset,
    folder: &Path,
    base: Option<&Path>,
) -> Result<Asset, MediaError> {
    let figures = if card.poses.is_empty() {
        card.expressions.len().min(5)
    } else {
        card.poses.len().min(5)
    }
    .max(1);
    let request = sheet_request(context, card, id, style, base)?;
    let path = folder.join(format!("{id}.png"));
    let mut asset = context
        .media
        .image(
            &format!("sheet-{id}"),
            context.guide.manifest().roles.image,
            &request,
            &path,
        )
        .await?;
    if context.guide.manifest().quality.image_qa {
        for attempt in 0..=1 {
            // The content hash keys the cached answer to this exact image.
            let mut question = format!(
                "The first image is a character reference sheet ({}); the second is the style reference. Expected: {}\nCount the full-body figures (figures). same_character: every figure has the same face, hair, build, outfit and colours as the design. Set text_present for any letters, numbers or labels. anatomy_errors: extra or missing arms, hands or fingers, melted faces. problems: anything that makes it unusable as a reference: wrong number of figures, a different outfit or colours from the design, cropped figures, expressions that do not differ, a background scene, or a clearly different art style.",
                asset.content_hash, request.prompt
            );
            // A row of identical heads reads as a text line to the local detector, so on a
            // sheet its finding is only a hint for the vision check, never a verdict.
            if let Some(hint) =
                crate::local_checks::printed_text(&asset.path, &context.media.cancel).await?
            {
                write!(
                    question,
                    "\nA local detector suspects {hint}. Inspect that area; set text_present only for real letters or numbers, not heads, hair or folds."
                )
                .map_err(|e| EngineError::Setup(e.to_string()))?;
            }
            let images = vec![asset.path.clone(), style.path.clone()];
            let qa: SheetQa = context
                .ask(
                    &format!("sheet-check-{id}-{attempt}"),
                    context.guide.manifest().roles.image_qa,
                    question.clone(),
                    images.clone(),
                )
                .await?;
            let qa = confirm(
                context,
                &format!("sheet-confirm-{id}-{attempt}"),
                qa,
                |q: &SheetQa| q.passes(figures),
                question,
                images,
            )
            .await?;
            if qa.passes(figures) {
                break;
            }
            if attempt == 1 {
                let consistent_figures = qa.figures == figures && qa.same_character;
                let clean_anatomy = !qa.text_present && qa.anatomy_errors.is_empty();
                if consistent_figures && clean_anatomy {
                    break;
                }
                return Err(MediaError::Quality(format!(
                    "character sheet for {} failed its check after regeneration: {}",
                    card.name,
                    serde_json::to_string(&qa).map_err(|e| EngineError::Setup(e.to_string()))?
                )));
            }
            let mut retry = request.clone();
            write!(
                retry.prompt,
                "\nThe previous sheet was rejected; fix every issue: {}",
                serde_json::to_string(&qa).map_err(|e| EngineError::Setup(e.to_string()))?
            )
            .map_err(|e| EngineError::Setup(e.to_string()))?;
            asset = context
                .media
                .image(
                    &format!("sheet-{id}-fix"),
                    context.guide.manifest().roles.image,
                    &retry,
                    &path,
                )
                .await?;
        }
    }
    Ok(asset)
}

/// Free checks for an empty place or prop reference: no printed text, no people.
async fn reference_problem(context: &Context, path: &Path) -> Result<Option<String>, MediaError> {
    if let Some(problem) = crate::local_checks::printed_text(path, &context.media.cancel).await? {
        return Ok(Some(problem));
    }
    crate::local_checks::people_in_empty_reference(path, &context.media.cancel).await
}

/// What a locked reference draws: a recurring place or a signature prop.
#[derive(Clone, Copy)]
enum Locked {
    Place,
    Prop,
}

impl Locked {
    const fn step(self) -> &'static str {
        match self {
            Self::Place => "place",
            Self::Prop => "prop",
        }
    }

    const fn brief(self) -> &'static str {
        match self {
            Self::Place => {
                "A wide establishing view of the empty place, showing its layout, architecture, light sources, colours and key furniture or landmarks clearly, as a set an illustrator returns to."
            }
            Self::Prop => {
                "The single object alone, large and centred on a plain white background with a small flat contact shadow, its shape, proportions, material and colours clear, as a prop an illustrator returns to."
            }
        }
    }
}

/// One people-free reference image per place or prop the shots use, so it is drawn the
/// same way in every frame. Shared like the cast; cached by description and style.
async fn locked_refs(
    context: &Context,
    kind: Locked,
    items: &[(String, String)],
    style: &Asset,
) -> Result<BTreeMap<String, Asset>, MediaError> {
    let step = kind.step();
    let folder = context.layout.cast_dir().join(format!("{step}s"));
    let _lock = shared_lock(&folder.join(format!(".{step}s.lock")), context).await?;
    let mut result = BTreeMap::new();
    for (name, description) in items {
        let id = Slug::from_title(name).map_err(|e| EngineError::Setup(e.to_string()))?;
        let key = hash(&(
            1,
            context
                .media
                .store
                .lock()
                .await
                .revision(&format!("{step}-{id}")),
            name,
            description,
            &style.content_hash,
        ))?;
        let cached = folder.join(format!("{id}-{key}.json"));
        let asset = if let Some(asset) = read_shared(&cached)? {
            asset
        } else {
            let mut request = ImageRequest {
                prompt: format!(
                    "{}\n{} reference for {name}: {}. {} No people, characters, faces, figures or animals: this image is copied into every frame that shows it, so anyone in it would appear in all of them. No text, signs with letters, borders or labels. Match the attached style reference's line work and rendering precisely.",
                    render_style(context)?,
                    if matches!(kind, Locked::Place) {
                        "Location"
                    } else {
                        "Prop"
                    },
                    description.trim(),
                    kind.brief()
                ),
                references: vec![style.path.clone()],
                aspect: context.guide.manifest().format.aspect.clone(),
                resolution: "2K".into(),
            };
            request.prompt = context.prompt(&format!("{step}-{id}"), &request.prompt)?;
            let path = folder.join(format!("{id}.png"));
            let image_tier = context.guide.manifest().roles.image;
            let mut asset = context
                .media
                .image(&format!("{step}-{id}"), image_tier, &request, &path)
                .await?;
            if let Some(problem) = reference_problem(context, &asset.path).await? {
                let mut retry = request.clone();
                write!(
                    retry.prompt,
                    "\nThe previous image was rejected: {problem}."
                )
                .map_err(|e| EngineError::Setup(e.to_string()))?;
                asset = context
                    .media
                    .image(&format!("{step}-{id}-fix"), image_tier, &retry, &path)
                    .await?;
            }
            fsx::write_json_atomic(&cached, &asset)?;
            asset
        };
        result.insert(name.clone(), asset);
    }
    fsx::write_json_atomic(
        &context
            .layout
            .stage(Stage::Images)
            .join(format!("{step}s.json")),
        &result,
    )?;
    Ok(result)
}

#[derive(Debug, Default)]
struct SheetProgress {
    assets: BTreeMap<String, Asset>,
    finished: bool,
    error: Option<String>,
}
async fn wait_sheets(
    context: &Context,
    names: &[String],
    style: &Path,
    sheets: &tokio::sync::Mutex<SheetProgress>,
) -> Result<Vec<PathBuf>, MediaError> {
    loop {
        context.check_cancel()?;
        let ready = sheets.lock().await;
        if names.iter().all(|name| ready.assets.contains_key(name)) {
            return Ok(std::iter::once(style.into())
                .chain(names.iter().map(|name| ready.assets[name].path.clone()))
                .collect());
        }
        if ready.finished {
            return Err(MediaError::Quality(
                ready
                    .error
                    .clone()
                    .unwrap_or_else(|| "required character sheet is missing".into()),
            ));
        }
        drop(ready);
        tokio::select! {()=context.media.cancel.cancelled()=>context.check_cancel()?,()=tokio::time::sleep(Duration::from_millis(100))=>{}}
    }
}

async fn frames(
    context: Arc<Context>,
    board: Arc<Storyboard>,
    style: Asset,
    sheets: Arc<tokio::sync::Mutex<SheetProgress>>,
    places: Arc<BTreeMap<String, Asset>>,
    budget: FrameBudget,
) -> Result<Vec<Frame>, MediaError> {
    let paneled = panel_frames(&context, &board, &style, &sheets, &places, budget).await?;
    let economy = budget == FrameBudget::Economy;
    let mut tasks = JoinSet::new();
    for (unit, shot) in board.units.iter().zip(&board.direction.shots) {
        if shot.reuse.is_some() || paneled.contains_key(&shot.unit) {
            continue;
        }
        let place = places.get(&shot.setting).map(|a| a.path.clone());
        let props: Vec<_> = shot
            .props
            .iter()
            .filter_map(|p| {
                places
                    .get(&format!("prop:{p}"))
                    .map(|a| (p.clone(), a.path.clone()))
            })
            .collect();
        let details = shot_details(shot, &board.plan, props.iter().map(|(n, _)| n.as_str()));
        let context = Arc::clone(&context);
        let unit = unit.clone();
        let shot = shot.clone();
        let style_path = style.path.clone();
        let sheets = Arc::clone(&sheets);
        tasks.spawn(async move {
            let mut refs=wait_sheets(&context,&shot.characters,&style_path,&sheets).await?;
            // Props come before the place, which stays the last reference as the prompt says.
            refs.extend(props.into_iter().map(|(_, path)| path));
            refs.extend(place);
            let mut prompt = scene_prompt(&context, &unit, &shot, &details)?;
            // Judge against the canonical scene, not prior criticisms or repair-only
            // composition hints, which would bias QA and invent additional gates.
            let qa_expectation=prompt.clone();
            let revision=context.media.store.lock().await.revision(&format!("image-{}-0",unit.id));
            let correction=redo_feedback(&context.layout,unit.id,revision)?;
            let mut previous=correction.as_ref().and_then(|c|c.previous_provider.clone());
            if let Some(correction)=correction {
                let qa=correction.qa;
                let zones=&context.guide.manifest().visuals.safe_zones;
                let rendered_style=render_style(&context)?.split_whitespace().collect::<Vec<_>>().join(" ");
                let styling=if rendered_style.contains("Oval heads, dot eyes") { "Use simple oval heads and small dot eyes, flat cel colours, broad uneven charcoal outlines rather than detailed hatching." } else { "Follow the guide's visual style and attached reference precisely." };
                write!(prompt,"\nThis is a targeted correction of a previously rejected frame. Correct every listed defect: {}. Recompose the entire relevant action, including hands, props and feet, inside normalized vertical coordinates {} to {}. Leave additional breathing room inside both boundaries; do not merely move the face. Keep lower caption space empty of relevant action. {} Preserve the attached character identity, hair and clothing. Make the specified action unmistakable.",serde_json::to_string(&qa).map_err(|e|EngineError::Setup(e.to_string()))?,zones.top,1.0-zones.bottom,styling).map_err(|e|EngineError::Setup(e.to_string()))?;
                if correction.precise_framing {
                    prompt.push_str("\nFraming takes priority over showing the whole body: preserve the specified shot size. Medium shots and close-ups must intentionally crop out legs and feet; do not widen them into a full-body composition. For wide shots showing feet, keep the entire figure small, in the upper-middle half of the canvas, with every boot above 65% image height and a large empty lower third. Keep visible hands and important props above that same 65% line. Make the precise action and any specified point of physical contact unmistakable rather than substituting another gesture. Do not add a second saturated accent or change the story.");
                }
                if let Some(extra)=correction.extra_instructions {prompt.push_str(&extra);}
            }
            let mut request = ImageRequest { prompt,references:refs,aspect:context.guide.manifest().format.aspect.clone(),resolution:if economy { "1K".into() } else { context.guide.manifest().visuals.frame_resolution.clone() } };
            let last_attempt = u32::from(!economy);
            let mut edit: Option<ImageRequest> = None;
            for attempt in 0..=last_attempt {
                let step=format!("image-{}-{attempt}",unit.id);let path=context.layout.stage(Stage::Images).join(format!("unit-{}.png",unit.id));
                let current = edit.take().unwrap_or_else(|| request.clone());
                let asset={
                    let drawn=if let Some(provider)=previous.as_deref(){context.media.repair_image(&step,context.guide.manifest().roles.image,&current,&path,provider).await}else{context.media.image(&step,context.guide.manifest().roles.image,&current,&path).await};
                    match drawn {
                        // Every provider declined: narration can carry sensitive wording the
                        // shot itself avoids, so try once with the shot description alone.
                        Err(MediaError::Unavailable(why)) if why.contains("without generating an image") || why.contains("stream disconnected") => {
                            let mut plain=current.clone();
                            plain.prompt=plain.prompt.replace(&format!(" Narration context: {}.",unit.narration),"");
                            plain.prompt.push_str("\nShow only the described action, non-explicit and suitable for all audiences.");
                            context.media.image(&step,context.guide.manifest().roles.image,&plain,&path).await?
                        }
                        other=>other?,
                    }
                };
                previous=Some(asset.provider.clone());
                let exposure = free_check(&context, &asset.path, &shot.characters).await?;
                let qa = if let Some(problem) = exposure {
                    ImageQa { style_score:0.0,character_score:0.0,text_present:false,anatomy_errors:vec![],problems:vec![problem],notes:vec![],focus:shot.focus }
                } else if context.guide.manifest().quality.image_qa {
                    let images: Vec<_> = std::iter::once(asset.path.clone()).chain(request.references.clone()).collect();
                    let question = qa_prompt(context.guide.manifest().visuals.safe_zones, &qa_expectation);
                    let qa: ImageQa = context.ask(&format!("qa-{}-{attempt}",unit.id),context.guide.manifest().roles.image_qa,question.clone(),images.clone()).await?;
                    confirm(&context, &format!("qa-confirm-{}-{attempt}",unit.id), qa, ImageQa::passes, question, images).await?
                } else { ImageQa { style_score:10.0,character_score:10.0,text_present:false,anatomy_errors:vec![],problems:vec![],notes:vec![],focus:shot.focus } };
                if qa.passes() || attempt==last_attempt { return Ok(Frame { unit:unit.id,image:asset,qa,regenerated:attempt > 0 }); }
                if qa.problems.len() == 1 && qa.anatomy_errors.is_empty() { edit = text_edit(&context, &asset, &request, unit.id).await?; }
                request.prompt.push_str("\nCorrect these QA failures: ");
                request.prompt.push_str(&serde_json::to_string(&qa).map_err(|e|EngineError::Setup(e.to_string()))?);
            }
            Err(MediaError::Quality(format!("unit {} failed image QA after regeneration",unit.id)))
        });
    }
    let mut frames: Vec<Frame> = paneled.into_values().collect();
    frames.extend(collect_frames(tasks).await?);
    let mut frames = recut_reused(&context, &board, frames).await?;
    frames.sort_by_key(|f| f.unit);
    Ok(frames)
}

async fn collect_frames(
    mut tasks: JoinSet<Result<Frame, MediaError>>,
) -> Result<Vec<Frame>, MediaError> {
    let mut frames = Vec::new();
    let mut failure = None;
    while let Some(result) = tasks.join_next().await {
        match result
            .map_err(|e| MediaError::from(EngineError::Setup(format!("image task failed: {e}"))))
        {
            Ok(Ok(frame)) => frames.push(frame),
            Ok(Err(error)) | Err(error) => {
                if failure.is_none() {
                    failure = Some(error);
                }
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(frames)
}

/// Recuts reused shots from their freshly drawn source frames, locally and for free.
async fn recut_reused(
    context: &Context,
    board: &Storyboard,
    mut frames: Vec<Frame>,
) -> Result<Vec<Frame>, MediaError> {
    for shot in board.direction.shots.iter().filter(|s| s.reuse.is_some()) {
        let source = frames
            .iter()
            .find(|f| Some(f.unit) == shot.reuse)
            .cloned()
            .ok_or_else(|| {
                MediaError::Quality(format!("unit {} reuses a missing frame", shot.unit))
            })?;
        let path = context
            .layout
            .stage(Stage::Images)
            .join(format!("unit-{}-recut.png", shot.unit));
        frames.push(recut(&source, shot, &path, &context.media.cancel).await?);
    }
    Ok(frames)
}

/// How much of the source frame a reused shot keeps, by shot size.
fn recut_scale(shot: &str) -> f64 {
    let shot = shot.to_lowercase();
    if shot.contains("close") {
        0.5
    } else if shot.contains("medium") {
        0.7
    } else {
        0.85
    }
}

/// Crops `source` around the shot's focus (keeping the aspect ratio) and scales it back to
/// full size, so a reaction or continuing moment needs no new generation.
async fn recut(
    source: &Frame,
    shot: &crate::story::Shot,
    path: &Path,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Frame, MediaError> {
    let image = image::open(&source.image.path).map_err(|e| {
        MediaError::Quality(format!(
            "{} is not a readable image: {e}",
            source.image.path.display()
        ))
    })?;
    let (w, h) = (image.width(), image.height());
    let scale = recut_scale(&shot.shot);
    let (cw, ch) = (
        num::round_u32(f64::from(w) * scale),
        num::round_u32(f64::from(h) * scale),
    );
    let place = |focus: f64, full: u32, part: u32| -> u32 {
        let start = num::trunc_u32(
            (focus.clamp(0.0, 1.0) * f64::from(full) - f64::from(part) / 2.0)
                .round()
                .max(0.0),
        );
        start.min(full - part)
    };
    let (x, y) = (place(shot.focus[0], w, cw), place(shot.focus[1], h, ch));
    let out = crate::local_checks::enlarge(image.crop_imm(x, y, cw, ch).to_rgb8(), (w, h), cancel)
        .await?;
    out.save(path)
        .map_err(|e| MediaError::Quality(format!("cannot save {}: {e}", path.display())))?;
    let bytes = std::fs::read(path).map_err(|e| EngineError::io(path, e))?;
    // The focus moves with the crop: it is now relative to the recut frame.
    let refocus = |focus: f64, start: u32, part: u32, full: u32| {
        ((focus * f64::from(full) - f64::from(start)) / f64::from(part)).clamp(0.0, 1.0)
    };
    let mut qa = source.qa.clone();
    qa.focus = [
        refocus(shot.focus[0], x, cw, w),
        refocus(shot.focus[1], y, ch, h),
    ];
    Ok(Frame {
        unit: shot.unit,
        image: Asset {
            path: path.into(),
            content_hash: blake3::hash(&bytes).to_hex().to_string(),
            provider: "local-recut".into(),
            model: format!("crop-of-unit-{}", source.unit),
            usage: retold_providers::media::MediaUsage {
                generation: None,
                cost: Some(retold_core::Usd::ZERO),
            },
            quote: retold_providers::media::Quote {
                ceiling: retold_core::Usd::ZERO,
                provider_tag: None,
            },
            request_id: String::new(),
        },
        qa,
        regenerated: false,
    })
}

/// What the image checker is asked. Only defects that would spoil the video go in
/// `problems` (they fail the frame); small deviations go in `notes`, so a good frame is not
/// regenerated, and paid for again, over an eye glint or a slightly wider shot.
/// Free, deterministic first pass before the model judges a frame: an image that is almost
/// entirely black or white is a failed generation, whatever the style. Dark scenes are fine;
/// only near-solid frames (mean luminance under 2% or over 98%) are rejected.
fn exposure_problem(path: &Path) -> Result<Option<String>, MediaError> {
    let image = image::open(path)
        .map_err(|e| {
            MediaError::Quality(format!("{} is not a readable image: {e}", path.display()))
        })?
        .to_rgb8();
    let report = retold_algo::vision::check_luminance(&image);
    Ok(if report.mean_luminance < 0.02 {
        Some("the frame is almost entirely black; generate a visible, lit scene".into())
    } else if report.mean_luminance > 0.98 {
        Some("the frame is almost entirely white; generate a complete illustrated scene".into())
    } else {
        None
    })
}

/// When the only defect is stray text the local detector can locate, an edit that erases
/// just those regions (frame + white mask), keeping the rest of an otherwise good frame,
/// instead of drawing the whole frame again.
async fn text_edit(
    context: &Context,
    asset: &Asset,
    request: &ImageRequest,
    unit: usize,
) -> Result<Option<ImageRequest>, MediaError> {
    let boxes = crate::local_checks::text_boxes(&asset.path, &context.media.cancel).await?;
    if boxes.is_empty() {
        return Ok(None);
    }
    let size = image::image_dimensions(&asset.path).map_err(|e| {
        MediaError::Quality(format!(
            "{} is not a readable image: {e}",
            asset.path.display()
        ))
    })?;
    let mask = context
        .layout
        .stage(Stage::Images)
        .join(format!("unit-{unit}-text-mask.png"));
    crate::local_checks::text_mask(&boxes, size)
        .save(&mask)
        .map_err(|e| MediaError::Quality(format!("cannot save {}: {e}", mask.display())))?;
    Ok(Some(ImageRequest {
        prompt: format!(
            "{}\nEdit this illustration. The first reference is the frame; the second is a white edit mask. Inside the white mask regions only, remove every letter, word, number and label and repaint the area so it continues the surrounding scene seamlessly. Preserve every unmasked pixel exactly: composition, characters, colours, line work and style. Add no new text or details.",
            render_style(context)?
        ),
        references: vec![asset.path.clone(), mask],
        aspect: request.aspect.clone(),
        resolution: request.resolution.clone(),
    }))
}

/// The canonical single-frame prompt for a shot; panel checks judge against it too.
fn scene_prompt(
    context: &Context,
    unit: &Unit,
    shot: &crate::story::Shot,
    details: &str,
) -> Result<String, MediaError> {
    let prompt = format!(
        "{}\nShot: {}. Location: {}. Action: {}. Mood: {}. Original characters: {:?}. Match each character's attached sheet precisely. Narration context: {}. Keep faces in the safe zone; no text or letters.{details}",
        render_style(context)?,
        shot.shot,
        shot.location,
        shot.action,
        shot.mood,
        shot.characters,
        unit.narration
    );
    context.prompt("image-scene", &prompt)
}

/// Prop names in a shot that have a drawn reference, with the reference path.
fn shot_props(shot: &crate::story::Shot, refs: &BTreeMap<String, Asset>) -> Vec<(String, PathBuf)> {
    shot.props
        .iter()
        .filter_map(|p| {
            refs.get(&format!("prop:{p}"))
                .map(|a| (p.clone(), a.path.clone()))
        })
        .collect()
}

/// Draws groups of supporting shots as multi-panel sheets (guide `visuals.panels`), splits
/// them and checks every panel like a single frame. Panels that fail, and sheets that cannot
/// be split, are left out, so those shots are drawn one by one as usual.
async fn panel_frames(
    context: &Arc<Context>,
    board: &Storyboard,
    style: &Asset,
    sheets: &Arc<tokio::sync::Mutex<SheetProgress>>,
    refs: &BTreeMap<String, Asset>,
    budget: FrameBudget,
) -> Result<BTreeMap<usize, Frame>, MediaError> {
    let m = context.guide.manifest();
    let mut out = BTreeMap::new();
    if m.visuals.panels < 2 {
        return Ok(out);
    }
    let redone = redone_units(context, board).await;
    let resolution = if budget == FrameBudget::Economy {
        "1K".to_owned()
    } else {
        m.visuals.frame_resolution.clone()
    };
    let (cols, rows, sheet_aspect) = crate::panels::grid(m.visuals.panels, &m.format.aspect);
    let size = crate::panels::frame_size(&resolution, &m.format.aspect);
    let folder = context.layout.stage(Stage::Images);
    for group in crate::panels::groups(&board.direction.shots, m.visuals.panels as usize, &|u| {
        redone.contains(&u)
    }) {
        let shots: Vec<_> = group.iter().map(|u| &board.direction.shots[*u]).collect();
        let mut references =
            wait_sheets(context, &shots[0].characters, &style.path, sheets).await?;
        let mut props: Vec<(String, PathBuf)> = Vec::new();
        for s in &shots {
            for p in shot_props(s, refs) {
                if !props.contains(&p) {
                    props.push(p);
                }
            }
        }
        references.extend(props.iter().map(|(_, path)| path.clone()));
        references.extend(refs.get(&shots[0].setting).map(|a| a.path.clone()));
        let request = panel_request(
            context,
            board,
            &shots,
            refs,
            references.clone(),
            (cols, rows, &sheet_aspect),
            &resolution,
        )?;
        let label = group
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("-");
        let sheet = context
            .media
            .image(
                &format!("panels-{label}"),
                m.roles.image,
                &request,
                &folder.join(format!("panels-{label}.png")),
            )
            .await?;
        let image = image::open(&sheet.path)
            .map_err(|e| {
                MediaError::Quality(format!(
                    "{} is not a readable image: {e}",
                    sheet.path.display()
                ))
            })?
            .to_rgb8();
        let Ok(parts) = crate::panels::split(&image, cols, rows, &m.format.aspect) else {
            context.media.store.lock().await.emit(
                "images",
                crate::events::Status::Running,
                None,
                Some(format!(
                    "panel sheet {label} has no clean gutters; drawing those shots one by one"
                )),
            )?;
            continue;
        };
        for (s, part) in shots.iter().zip(parts) {
            let part = crate::local_checks::enlarge(part, size, &context.media.cancel).await?;
            if let Some(frame) =
                check_panel(context, board, s, &part, &sheet, &label, refs, &references).await?
            {
                out.insert(s.unit, frame);
            }
        }
    }
    Ok(out)
}

/// Units whose frame is being redone; they are drawn alone with their correction.
async fn redone_units(context: &Context, board: &Storyboard) -> std::collections::BTreeSet<usize> {
    let store = context.media.store.lock().await;
    board
        .direction
        .shots
        .iter()
        .filter(|s| store.revision(&format!("image-{}-0", s.unit)) > 0)
        .map(|s| s.unit)
        .collect()
}

/// The image request for one multi-panel sheet: every shot's description in grid order.
fn panel_request(
    context: &Context,
    board: &Storyboard,
    shots: &[&crate::story::Shot],
    refs: &BTreeMap<String, Asset>,
    references: Vec<PathBuf>,
    (cols, rows, sheet_aspect): (u32, u32, &str),
    resolution: &str,
) -> Result<ImageRequest, MediaError> {
    let mut panels_text = String::new();
    for (i, s) in shots.iter().enumerate() {
        let names: Vec<_> = shot_props(s, refs).into_iter().map(|(n, _)| n).collect();
        let details = shot_details(s, &board.plan, names.iter().map(String::as_str));
        let _ = write!(
            panels_text,
            "\nPanel {} (row {}, column {}): {} shot. Location: {}. Action: {}. Mood: {}.{details}",
            i + 1,
            i / cols as usize + 1,
            i % cols as usize + 1,
            s.shot,
            s.location,
            s.action,
            s.mood
        );
    }
    let prompt = format!(
        "{}\nA storyboard sheet of exactly {} separate illustrated panels in a {cols}-column, {rows}-row grid, divided by plain pure-white gutters about 3% of the image wide running edge to edge. No borders, panel numbers, captions, letters or text anywhere. Each panel is a complete full-bleed frame of the same scene, with identical characters, outfits, lighting and setting across panels; each composition fills its own cell and keeps faces away from the cell's top and bottom edges. Original characters: {:?}. Match each character's attached sheet precisely.{panels_text}",
        render_style(context)?,
        shots.len(),
        shots[0].characters
    );
    Ok(ImageRequest {
        prompt: context.prompt("image-scene", &prompt)?,
        references,
        aspect: sheet_aspect.to_owned(),
        resolution: resolution.to_owned(),
    })
}

/// Saves one split panel and checks it like a single frame; `None` when it fails, so the
/// shot is drawn on its own instead.
#[expect(
    clippy::too_many_arguments,
    reason = "split pending (Phase 0 step 3c); counted by the clean-code baseline"
)]
async fn check_panel(
    context: &Context,
    board: &Storyboard,
    s: &crate::story::Shot,
    part: &image::RgbImage,
    sheet: &Asset,
    label: &str,
    refs: &BTreeMap<String, Asset>,
    references: &[PathBuf],
) -> Result<Option<Frame>, MediaError> {
    let m = context.guide.manifest();
    let folder = context.layout.stage(Stage::Images);
    let path = folder.join(format!("unit-{}-panel.png", s.unit));
    part.save(&path)
        .map_err(|e| MediaError::Quality(format!("cannot save {}: {e}", path.display())))?;
    let bytes = std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?;
    let asset = Asset {
        path: path.clone(),
        content_hash: blake3::hash(&bytes).to_hex().to_string(),
        provider: sheet.provider.clone(),
        model: format!("{} (panel of {label})", sheet.model),
        usage: retold_providers::media::MediaUsage {
            generation: None,
            cost: Some(retold_core::Usd::ZERO),
        },
        quote: retold_providers::media::Quote {
            ceiling: retold_core::Usd::ZERO,
            provider_tag: None,
        },
        request_id: String::new(),
    };
    if free_check(context, &path, &s.characters).await?.is_some() {
        return Ok(None);
    }
    let names: Vec<_> = shot_props(s, refs).into_iter().map(|(n, _)| n).collect();
    let expectation = scene_prompt(
        context,
        &board.units[s.unit],
        s,
        &shot_details(s, &board.plan, names.iter().map(String::as_str)),
    )?;
    let qa = if m.quality.image_qa {
        // The content hash keys the cached verdict to this exact panel.
        let question = format!(
            "{} (panel {})",
            qa_prompt(m.visuals.safe_zones, &expectation),
            asset.content_hash
        );
        let images: Vec<_> = std::iter::once(path.clone())
            .chain(references.iter().cloned())
            .collect();
        let qa: ImageQa = context
            .ask(
                &format!("qa-panel-{}", s.unit),
                m.roles.image_qa,
                question.clone(),
                images.clone(),
            )
            .await?;
        confirm(
            context,
            &format!("qa-panel-confirm-{}", s.unit),
            qa,
            ImageQa::passes,
            question,
            images,
        )
        .await?
    } else {
        ImageQa {
            style_score: 10.0,
            character_score: 10.0,
            text_present: false,
            anatomy_errors: vec![],
            problems: vec![],
            notes: vec![],
            focus: s.focus,
        }
    };
    Ok(qa.passes().then_some(Frame {
        unit: s.unit,
        image: asset,
        qa,
        regenerated: false,
    }))
}

/// The parts of a frame prompt that lock it to the story: props, continuity, setting and
/// each character's sheet expression.
fn shot_details<'a>(
    shot: &crate::story::Shot,
    plan: &Plan,
    props: impl Iterator<Item = &'a str>,
) -> String {
    let mut out = String::new();
    let props: Vec<_> = props.collect();
    if !props.is_empty() {
        let _ = write!(
            out,
            " Props in this shot, each matching its own attached reference image (after the character sheets): {}.",
            props.join(", ")
        );
    }
    if !shot.continuity.trim().is_empty() {
        let _ = write!(
            out,
            " Continuity (must be visible and match the surrounding shots): {}.",
            shot.continuity.trim()
        );
    }
    if let Some(p) = plan.settings.iter().find(|p| p.name == shot.setting) {
        let _ = write!(
            out,
            " Setting: {} — {}. The LAST attached image is this location's reference: keep its layout, architecture, colours and key furniture, seen from this shot's angle.",
            p.name,
            p.description.trim()
        );
    }
    if !shot.poses.is_empty() {
        let poses: Vec<_> = shot.poses.iter().map(|p| format!("{} shows {} (copy the brows, eyes, mouth and body language of the matching pose on their sheet)", p.name, p.expression.to_uppercase())).collect();
        let _ = write!(
            out,
            " Expressions: {}. Draw each character once.",
            poses.join("; ")
        );
    }
    out
}

/// Free checks before any model is asked: exposure, locked outfit colours, then printed
/// text with the local detector when it is installed.
async fn free_check(
    context: &Context,
    path: &Path,
    characters: &[String],
) -> Result<Option<String>, MediaError> {
    if let Some(problem) = exposure_problem(path)? {
        return Ok(Some(problem));
    }
    if let Some(problem) = palette_problem(path, characters, &frame_cards(context)?)? {
        return Ok(Some(problem));
    }
    crate::local_checks::printed_text(path, &context.media.cancel).await
}

pub(crate) fn read_cards(path: &Path) -> Result<BTreeMap<String, Character>, MediaError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            EngineError::Config {
                path: path.into(),
                message: e.to_string(),
            }
            .into()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(EngineError::io(path, e).into()),
    }
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let h = hex.trim().strip_prefix('#')?;
    let [_, r, g, b] = u32::from_str_radix(h, 16)
        .ok()
        .filter(|_| h.len() == 6)?
        .to_be_bytes();
    Some([r, g, b])
}

/// Free check before the paid one: a character whose locked clothing colours appear nowhere
/// in the frame was almost certainly redrawn in other clothes. Deliberately lenient (wide
/// tolerance, any one garment colour suffices, skin and hair ignored) so style shifts pass.
fn palette_problem(
    path: &Path,
    characters: &[String],
    cards: &BTreeMap<String, Character>,
) -> Result<Option<String>, MediaError> {
    let mut image = None;
    for name in characters {
        let Some(card) = cards.get(name) else {
            continue;
        };
        let colors: Vec<_> = card
            .colors
            .iter()
            .filter(|s| {
                !["skin", "hair", "eyes"]
                    .iter()
                    .any(|x| s.part.to_lowercase().contains(x))
            })
            .filter_map(|s| parse_hex(&s.hex))
            .collect();
        if colors.is_empty() {
            continue;
        }
        let img = match &mut image {
            Some(img) => &*img,
            slot => slot.insert(
                image::open(path)
                    .map_err(|e| {
                        MediaError::Quality(format!(
                            "{} is not a readable image: {e}",
                            path.display()
                        ))
                    })?
                    .to_rgb8(),
            ),
        };
        if colors
            .iter()
            .all(|c| retold_algo::vision::color_coverage(img, *c, 60.0) < 0.0005)
        {
            return Ok(Some(format!(
                "{name}'s outfit colours ({}) appear nowhere in the frame; draw them exactly as on their sheet",
                card.colors
                    .iter()
                    .map(|s| format!("{} {}", s.part, s.hex))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    Ok(None)
}

/// A failed check from the fast vision tier is confirmed by the smart tier before anything
/// is redrawn: a check costs far less than a paid regeneration, and fast checkers reject
/// good frames more often. Passing results, and smart-tier results, are final.
async fn confirm<T: Serialize + serde::de::DeserializeOwned + JsonSchema>(
    context: &Context,
    step: &str,
    first: T,
    passes: impl Fn(&T) -> bool,
    question: String,
    images: Vec<PathBuf>,
) -> Result<T, MediaError> {
    let tier = context.guide.manifest().roles.image_qa;
    if passes(&first) || tier == retold_core::Tier::VisionSmart {
        return Ok(first);
    }
    match context
        .ask::<T>(step, retold_core::Tier::VisionSmart, question, images)
        .await
    {
        Ok(second) => Ok(second),
        // No smart route (or it failed): keep the fast verdict rather than block the run.
        Err(MediaError::Unavailable(_)) => Ok(first),
        Err(e) => Err(e),
    }
}

fn qa_prompt(zones: retold_guide::manifest::SafeZones, expectation: &str) -> String {
    format!(
        "Inspect the FIRST image against the style and character references that follow it. \
         Expected shot and action: {expectation}.\n\
         Score style_score and character_score from 0 to 10 (7 or more is acceptable). If no \
         character is required, character_score is 10.\n\
         problems: list ONLY defects that make the frame unusable in the video:\n\
         1. letters, words, numbers, logos or watermarks anywhere in the image (also set text_present);\n\
         2. a required character missing, shown twice, or clearly not the person on their reference \
            sheet (different outfit, colours, hair or face);\n\
         3. malformed anatomy: extra or missing fingers or limbs, broken or melted faces (also list in \
            anatomy_errors);\n\
         4. a relevant face, or the key action, above {top:.2} or below {bottom:.2} of the image height, \
            where the title and captions cover it;\n\
         5. a clearly different art style from the references, such as photorealism or 3D rendering;\n\
         6. an action, a character's expression, or a stated continuity detail (time of day, light, injuries, clothing) that contradicts the expected one.\n\
         notes: list smaller deviations that do NOT make the frame unusable, for example a shot a \
         little wider or tighter than asked, eye or hand details, a repeated accent colour, or a \
         slightly different mood. Never put these in problems.\n\
         focus: the normalized [x, y] of the main subject. An empty problems list means the frame passes.",
        top = zones.top,
        bottom = 1.0 - zones.bottom,
    )
}

/// Freeze feedback for a redo revision, so replacing the aggregate cannot change
/// that revision's prompt on resume and accidentally regenerate a paid asset.
fn redo_feedback(
    layout: &crate::RunLayout,
    unit: usize,
    revision: u64,
) -> Result<Option<Correction>, EngineError> {
    if revision == 0 {
        return Ok(None);
    }
    let path = layout
        .meta()
        .join("image-feedback")
        .join(format!("{unit}-{revision}.json"));
    if path.exists() {
        let value: serde_json::Value = read_feedback_json(&path)?;
        if value.get("qa").is_some() {
            let saved: RedoFeedback =
                serde_json::from_value(value).map_err(|e| EngineError::Config {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
            return Ok(saved.qa.map(|qa| Correction {
                qa,
                precise_framing: saved.precise_framing,
                extra_instructions: saved.extra_instructions,
                previous_provider: saved.previous_provider,
            }));
        }
        let qa: Option<ImageQa> =
            serde_json::from_value(value).map_err(|e| EngineError::Config {
                path: path.clone(),
                message: e.to_string(),
            })?;
        return Ok(qa.map(|qa| Correction {
            qa,
            precise_framing: false,
            extra_instructions: None,
            previous_provider: None,
        }));
    }
    let aggregate = layout.stage(Stage::Images).join("images.json");
    let feedback = if aggregate.exists() {
        let images: Images = read_feedback_json(&aggregate)?;
        images
            .frames
            .into_iter()
            .find(|frame| frame.unit == unit && !frame.qa.passes())
            .map(|frame| (frame.qa, frame.image.provider))
    } else {
        None
    };
    fsx::write_json_atomic(
        &path,
        &RedoFeedback {
            qa: feedback.as_ref().map(|(qa, _)| qa.clone()),
            precise_framing: true,
            extra_instructions: feedback.as_ref().map(|(qa, _)| repair_details(qa)),
            previous_provider: feedback.as_ref().map(|(_, provider)| provider.clone()),
        },
    )?;
    Ok(feedback.map(|(qa, provider)| Correction {
        extra_instructions: Some(repair_details(&qa)),
        qa,
        precise_framing: true,
        previous_provider: Some(provider),
    }))
}

struct Correction {
    qa: ImageQa,
    precise_framing: bool,
    extra_instructions: Option<String>,
    previous_provider: Option<String>,
}

fn repair_details(qa: &ImageQa) -> String {
    let mut details="\nThe first attached image is a style reference only: copy its line work, palette and paper grain, never its people, clothing, props or setting. Each character's face, hair and outfit come only from their own character sheet. A sheet may show one character several times with different expressions: draw that character once, with the expression this frame needs.\nUse character references for identity attributes—hair silhouette, clothing, palette and distinguishing marks. The written guide controls graphical rendering: if the references contain finer shading, elaborate eyes or angular anatomy, simplify those to the guide's specified face and eye shapes, flat fills and broad outlines while preserving identity. Avoid decorative hatching. Important props and visible hands must be comfortably inside the safe zone, not touching its boundaries.".to_owned();
    let problems = qa.problems.join(" ").to_lowercase();
    if problems.contains("blood") && (problems.contains("neck") || problems.contains("kiss")) {
        details.push_str("\nFor this blood-drinking action, depict a clear side view of the vampire's mouth and small fangs contacting the SIDE OF THE NECK below the jaw, well away from the victim's lips and cheek. Tilt the victim's face away and upward so the exposed neck is clearly visible. The vampire approaches the exposed side of the neck from behind the shoulder; the pose must not resemble a kiss. Crop at the shoulders or chest for a close-up; only hands needed for the action may enter the frame. No graphic gore or second saturated accent.");
    }
    details
}

#[derive(Serialize, Deserialize)]
struct RedoFeedback {
    #[serde(default)]
    previous_provider: Option<String>,
    qa: Option<ImageQa>,
    precise_framing: bool,
    #[serde(default)]
    extra_instructions: Option<String>,
}

fn read_feedback_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, EngineError> {
    let bytes = std::fs::read(path).map_err(|e| EngineError::io(path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| EngineError::Config {
        path: path.into(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn locked_card() -> Character {
        Character {
            name: "Vera".into(),
            appearance: "Slim young teacher with a neat bob".into(),
            clothes: "Sage-green 1930s day dress with a white collar".into(),
            palette: vec!["#8FA98A".into()],
            distinguishing_detail: "Pearl stud earrings".into(),
            expressions: vec!["calm".into(), "scared".into()],
            age: "late twenties".into(),
            build: "7.5 heads tall, slim, upright".into(),
            face: "Oval head, dot eyes, thin arched brows".into(),
            hair: "Chin-length light brown waved bob".into(),
            voice: "female, late twenties, clear and controlled".into(),
            colors: vec![
                Swatch {
                    part: "skin".into(),
                    hex: "#F6E4D6".into(),
                },
                Swatch {
                    part: "hair".into(),
                    hex: "#9C7350".into(),
                },
                Swatch {
                    part: "dress".into(),
                    hex: "#8FA98A".into(),
                },
            ],
            poses: vec![
                Pose {
                    expression: "calm".into(),
                    face: "level brows, dot eyes, small neutral mouth".into(),
                    body: "upright, both hands clasped at the waist".into(),
                },
                Pose {
                    expression: "Scared".into(),
                    face: "raised brows, wide eyes, open mouth".into(),
                    body: "leaning back, both hands pressed to her chest".into(),
                },
            ],
        }
    }

    #[test]
    fn locked_cards_pass_and_vague_ones_are_named() {
        assert_eq!(design_problems(&locked_card()), Vec::<String>::new());
        let mut vague = locked_card();
        vague.build = "slim".into();
        vague.colors[0].hex = "pale".into();
        vague.poses.pop();
        vague.poses[0].body = "standing".into();
        let problems = design_problems(&vague).join("\n");
        for want in [
            "build",
            "heads",
            "colors",
            "one pose per expression",
            "both hands",
        ] {
            assert!(problems.contains(want), "{want} missing from {problems}");
        }
    }

    #[test]
    fn sheet_prompt_lists_the_locked_design_and_each_pose() {
        let prompt = sheet_prompt("STYLE", &locked_card());
        assert!(prompt.starts_with("STYLE\n"));
        assert!(prompt.contains("dress #8FA98A"));
        assert!(prompt.contains("exactly 2 times"));
        assert!(prompt.contains("2. SCARED — raised brows"));
        assert!(prompt.contains("7.5 heads tall"));
    }
    #[test]
    fn check_results_without_notes_serialize_exactly_as_before() {
        let old = r#"{"style_score":9.0,"character_score":9.0,"text_present":false,"anatomy_errors":[],"problems":["feet overlap captions"],"focus":[0.5,0.5]}"#;
        let qa: ImageQa = serde_json::from_str(old).unwrap();
        assert_eq!(serde_json::to_string(&qa).unwrap(), old);
    }

    #[test]
    fn redo_feedback_stays_frozen_when_the_aggregate_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = crate::RunLayout::from_video_dir(
            tmp.path(),
            "channel",
            &tmp.path().join("channel/films/example"),
            2,
        )
        .unwrap();
        let asset = json!({"path":"image.png","content_hash":"hash","provider":"fixture","model":"fixture","usage":{"cost":0,"generation":null},"quote":{"ceiling":0,"provider_tag":null},"request_id":"fixture"});
        let qa = json!({"style_score":9.0,"character_score":9.0,"text_present":false,"anatomy_errors":[],"problems":["feet overlap captions"],"focus":[0.5,0.5]});
        let mut images = json!({"cover":asset,"style":asset,"sheets":{},"frames":[{"unit":0,"image":asset,"qa":qa,"regenerated":true}]});
        let aggregate = layout.stage(Stage::Images).join("images.json");
        fsx::write_json_atomic(&aggregate, &images).unwrap();
        assert!(redo_feedback(&layout, 0, 0).unwrap().is_none());
        assert!(
            redo_feedback(&layout, 0, 1)
                .unwrap()
                .unwrap()
                .precise_framing
        );
        assert_eq!(
            redo_feedback(&layout, 0, 1).unwrap().unwrap().qa.problems,
            vec!["feet overlap captions"]
        );
        images["frames"][0]["qa"]["problems"] = json!([]);
        fsx::write_json_atomic(&aggregate, &images).unwrap();
        assert_eq!(
            redo_feedback(&layout, 0, 1).unwrap().unwrap().qa.problems,
            vec!["feet overlap captions"]
        );
        assert!(redo_feedback(&layout, 0, 2).unwrap().is_none());
        let legacy = layout.meta().join("image-feedback/0-3.json");
        fsx::write_json_atomic(&legacy, &qa).unwrap();
        let recovered = redo_feedback(&layout, 0, 3).unwrap().unwrap();
        assert_eq!(recovered.qa.problems, vec!["feet overlap captions"]);
        assert!(
            !recovered.precise_framing,
            "legacy prompt must stay unchanged on resume"
        );
    }
}

#[cfg(test)]
mod exposure_tests {
    use super::*;

    fn png(dir: &Path, name: &str, pixel: impl Fn(u32, u32) -> [u8; 3]) -> PathBuf {
        let image = image::RgbImage::from_fn(64, 64, |x, y| image::Rgb(pixel(x, y)));
        let path = dir.join(name);
        image.save(&path).unwrap();
        path
    }

    #[test]
    fn flags_a_character_whose_outfit_colours_are_absent() {
        let dir = tempfile::tempdir().unwrap();
        let green = png(dir.path(), "green.png", |x, _| {
            if x < 32 {
                [0x8F, 0xA9, 0x8A]
            } else {
                [250, 250, 250]
            }
        });
        let red = png(dir.path(), "red.png", |_, _| [0xB3, 0x20, 0x2A]);
        let cards = BTreeMap::from([("Vera".to_owned(), super::tests::locked_card())]);
        let vera = ["Vera".to_owned()];
        assert_eq!(palette_problem(&green, &vera, &cards).unwrap(), None);
        assert!(
            palette_problem(&red, &vera, &cards)
                .unwrap()
                .unwrap()
                .contains("dress #8FA98A")
        );
        assert_eq!(palette_problem(&red, &[], &cards).unwrap(), None);
    }

    #[test]
    fn recut_crops_around_the_focus_at_full_size() {
        let dir = tempfile::tempdir().unwrap();
        // Left half black, right half white: a close recut on the right is all white.
        let source = png(dir.path(), "src.png", |x, _| {
            if x < 32 { [0, 0, 0] } else { [255, 255, 255] }
        });
        let frame = Frame {
            unit: 0,
            image: Asset {
                path: source,
                content_hash: String::new(),
                provider: "p".into(),
                model: "m".into(),
                usage: retold_providers::media::MediaUsage {
                    generation: None,
                    cost: None,
                },
                quote: retold_providers::media::Quote {
                    ceiling: retold_core::Usd::ZERO,
                    provider_tag: None,
                },
                request_id: String::new(),
            },
            qa: ImageQa {
                style_score: 9.0,
                character_score: 9.0,
                text_present: false,
                anatomy_errors: vec![],
                problems: vec![],
                notes: vec![],
                focus: [0.5, 0.5],
            },
            regenerated: false,
        };
        let shot: crate::story::Shot = serde_json::from_value(serde_json::json!({"unit":1,"shot":"close","location":"l","action":"a","mood":"m","characters":[],"focus":[0.9,0.5],"reuse":0})).unwrap();
        let out = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(recut(
                &frame,
                &shot,
                &dir.path().join("out.png"),
                &tokio_util::sync::CancellationToken::new(),
            ))
            .unwrap();
        let img = image::open(&out.image.path).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (64, 64));
        assert!(img.pixels().all(|p| p.0[0] > 200));
        assert_eq!(out.unit, 1);
        assert_eq!(out.image.usage.cost, Some(retold_core::Usd::ZERO));
    }

    #[test]
    fn rejects_only_near_solid_frames() {
        let tmp = tempfile::tempdir().unwrap();
        let black = png(tmp.path(), "black.png", |_, _| [0, 0, 0]);
        let white = png(tmp.path(), "white.png", |_, _| [255, 255, 255]);
        // A dark, moody scene: mostly shadow with a lit figure.
        let noir = png(tmp.path(), "noir.png", |x, y| {
            if (20..44).contains(&x) && (10..54).contains(&y) {
                [200, 190, 170]
            } else {
                [4, 4, 6]
            }
        });
        assert!(exposure_problem(&black).unwrap().unwrap().contains("black"));
        assert!(exposure_problem(&white).unwrap().unwrap().contains("white"));
        assert_eq!(exposure_problem(&noir).unwrap(), None);
    }
}
