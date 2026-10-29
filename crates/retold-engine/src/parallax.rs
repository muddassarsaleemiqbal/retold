//! Real depth and background plates are durable items, independent of language timing.
use crate::{
    EngineError,
    assets::{Frame, Storyboard},
    context::Context,
    fsx,
    media_runtime::Asset,
    store::hash,
};
use retold_core::Tier;
use retold_parallax::depth::DepthMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthAsset {
    pub path: PathBuf,
    pub content_hash: String,
    pub model: String,
}
impl DepthAsset {
    pub fn load(&self) -> Result<DepthMap, EngineError> {
        let bytes = std::fs::read(&self.path).map_err(|e| EngineError::io(&self.path, e))?;
        if blake3::hash(&bytes).to_hex().as_str() != self.content_hash {
            return Err(EngineError::Setup(
                "depth cache integrity failed; redo the selected depth item".into(),
            ));
        }
        DepthMap::load(&self.path).map_err(|e| EngineError::Setup(e.to_string()))
    }
}
pub async fn depth(context: &Context, frame: &Frame) -> Result<DepthAsset, EngineError> {
    frame
        .image
        .verify()
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let step = format!("depth-{}", frame.unit);
    let model = context
        .depth
        .identity()
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let revision = context.media.store.lock().await.revision(&step);
    let inputs = serde_json::json!({"image":frame.image.content_hash,"model":model});
    let key = hash(&serde_json::json!({"version":1,"revision":revision,"inputs":inputs}))?;
    let cached: Option<DepthAsset> = context.media.store.lock().await.cached(&step, &key)?;
    if let Some(asset) = cached
        && asset.load().is_ok()
    {
        return Ok(asset);
    }
    let path = context
        .layout
        .video_dir()
        .join("work/depth")
        .join(format!("{key}.rgd"));
    let metadata = path.with_extension("json");
    if let Ok(bytes) = std::fs::read(&metadata)
        && let Ok(asset) = serde_json::from_slice::<DepthAsset>(&bytes)
        && asset.model == model
        && asset.path == path
        && asset.load().is_ok()
    {
        context
            .media
            .store
            .lock()
            .await
            .commit(&step, &key, &asset, None)?;
        return Ok(asset);
    }
    context.media.store.lock().await.begin(&step)?;
    let result = context
        .depth
        .estimate(&frame.image.path, &context.media.cancel)
        .await
        .map_err(|e| {
            EngineError::Setup(format!(
                "{e}; run `retold models verify depth`, then resume; log: {}",
                context.layout.meta().join("events.log").display()
            ))
        });
    let map = match result {
        Ok(m) => m,
        Err(e) => {
            context
                .media
                .store
                .lock()
                .await
                .task_stopped(&step, &e.to_string())?;
            return Err(e);
        }
    };
    map.save(&path)
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let asset = DepthAsset {
        content_hash: blake3::hash(&std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?)
            .to_hex()
            .to_string(),
        path,
        model,
    };
    fsx::write_json_atomic(&metadata, &asset)?;
    context
        .media
        .store
        .lock()
        .await
        .commit(&step, &key, &asset, None)?;
    Ok(asset)
}
/// Return an opt-in parallax request; explicit S1 shot overrides take precedence at the caller.
pub async fn request(
    context: &Context,
    board: &Storyboard,
    frame: &Frame,
    frames: u32,
) -> Result<Option<retold_parallax::render::Request>, EngineError> {
    let settings = &context.guide.manifest().visuals.parallax;
    let shot = board
        .direction
        .shots
        .get(frame.unit)
        .ok_or_else(|| EngineError::Setup("parallax shot is outside the storyboard".into()))?;
    if !settings.enabled
        || settings.strength <= 0.0
        || shot.motion != Some(crate::story::ShotMotion::Parallax)
    {
        return Ok(None);
    }
    let selected = board
        .direction
        .shots
        .iter()
        .filter(|s| s.motion == Some(crate::story::ShotMotion::Parallax))
        .count();
    let share = f64::from(u32::try_from(selected).map_err(|e| EngineError::Setup(e.to_string()))?)
        / f64::from(
            u32::try_from(board.direction.shots.len().max(1))
                .map_err(|e| EngineError::Setup(e.to_string()))?,
        );
    if share > settings.max_share + f64::EPSILON {
        return Err(EngineError::Setup(
            "saved storyboard exceeds visuals.parallax.max_share; redo storyboard".into(),
        ));
    }
    let depth = depth(context, frame).await?;
    let background = if shot.hero && settings.hero_fill {
        Some(background(context, frame, &depth).await?.path)
    } else {
        None
    };
    Ok(Some(retold_parallax::render::Request {
        image: frame.image.path.clone(),
        depth: depth.path,
        background,
        motion: shot
            .parallax_move
            .unwrap_or(crate::story::ParallaxMove::PushIn)
            .into(),
        strength: settings.strength,
        frames,
        fps: context.guide.manifest().format.fps.get(),
        attached: vec![],
    }))
}
async fn background(
    context: &Context,
    frame: &Frame,
    depth: &DepthAsset,
) -> Result<Asset, EngineError> {
    if !context.media.allow_paid {
        return Err(EngineError::Setup("hero_fill requires --allow-paid and sufficient budget; disable hero_fill for local-only parallax".into()));
    }
    let map = depth.load()?;
    let folder = context.layout.video_dir().join("work/parallax-background");
    let mask_path = folder.join(format!("{}.mask.png", depth.content_hash));
    let mut mask = image::GrayImage::new(map.width, map.height);
    for (i, p) in mask.pixels_mut().enumerate() {
        p[0] = if map.values[i] >= 0.55 { 255 } else { 0 };
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(mask.clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    fsx::write_atomic(&mask_path, &png.into_inner())?;
    let step = format!("depth-fill-{}", frame.unit);
    let generated=context.media.image(&step,Tier::Image,&retold_providers::media::ImageRequest{
        prompt:"Create a clean background plate for this illustration. The first reference is the original scene; the second is a white edit mask. Remove foreground subjects only in white mask regions, reconstruct the revealed scenery consistently with adjacent background, and preserve all unmasked pixels, perspective and style. No new characters, text or details outside the mask.".into(),
        references:vec![frame.image.path.clone(),mask_path],aspect:context.guide.manifest().format.aspect.clone(),resolution:format!("{}x{}",map.width,map.height),
    },&folder.join(format!("{}.generated.png",depth.content_hash))).await.map_err(|e|EngineError::Setup(e.to_string()))?;
    generated
        .verify()
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let original = retold_parallax::load_image(&frame.image.path)
        .map_err(|e| EngineError::Setup(e.to_string()))?
        .into_rgba8();
    let fill = retold_parallax::load_image(&generated.path)
        .map_err(|e| EngineError::Setup(e.to_string()))?
        .into_rgba8();
    let fill = image::imageops::resize(
        &fill,
        map.width,
        map.height,
        image::imageops::FilterType::Lanczos3,
    );
    let mut plate = original;
    for (x, y, p) in plate.enumerate_pixels_mut() {
        if mask.get_pixel(x, y)[0] > 0 {
            *p = *fill.get_pixel(x, y);
        }
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(plate)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| EngineError::Setup(e.to_string()))?;
    let bytes = png.into_inner();
    let path = folder.join(format!("{}.plate.png", blake3::hash(&bytes).to_hex()));
    fsx::write_atomic(&path, &bytes)?;
    Ok(Asset {
        path,
        content_hash: blake3::hash(&bytes).to_hex().to_string(),
        ..generated
    })
}
