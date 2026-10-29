//! Aspect editions and durable local package metadata; no platform writes.
use crate::{
    EngineError,
    context::Context,
    fsx,
    languages::{Edition, edition_id},
    media_runtime::MediaError,
};
use retold_deliver::{Aspect, Chapter, Post};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn formats(
    context: &Context,
    edition: &Edition,
) -> Result<Vec<(String, Option<Aspect>)>, MediaError> {
    let id = edition_id(&edition.variant);
    let values = &context.guide.manifest().delivery.aspects;
    if values.is_empty() {
        return Ok(vec![(id, None)]);
    }
    values
        .iter()
        .map(|v| {
            let aspect = Aspect::parse(v).map_err(setup)?;
            Ok((format!("{id}-{}", aspect.id()), Some(aspect)))
        })
        .collect()
}

pub fn aspect_context(context: &Context, aspect: Aspect) -> Result<Context, MediaError> {
    let mut result = context.clone();
    let mut files = result.guide.files().clone();
    let mut guide: toml::Value = toml::from_str(&files["guide.toml"]).map_err(setup)?;
    guide["format"]["aspect"] = aspect.ratio().into();
    guide["format"]["resolution"] =
        toml::Value::Array(aspect.dimensions().into_iter().map(Into::into).collect());
    files.insert("guide.toml".into(), toml::to_string(&guide).map_err(setup)?);
    result.guide =
        retold_guide::Guide::from_files(retold_guide::Origin::Inline, files).map_err(setup)?;
    Ok(result)
}

pub fn chapters(edition: &Edition) -> Result<Vec<Chapter>, MediaError> {
    if edition.units.len() != edition.alignment.units.len() {
        return Err(setup("chapter alignment mismatch"));
    }
    let mut chapters = Vec::new();
    for (i, (unit, timing)) in edition
        .units
        .iter()
        .zip(&edition.alignment.units)
        .enumerate()
    {
        if i == 0 || edition.units[i - 1].section != unit.section {
            chapters.push(Chapter {
                seconds: if i == 0 { 0.0 } else { timing.start },
                title: unit.section.clone(),
            });
        }
    }
    retold_deliver::validate_chapters(&chapters, edition.alignment.duration).map_err(setup)?;
    Ok(chapters)
}

pub async fn metadata(
    context: &Context,
    edition: &Edition,
    directory: &Path,
) -> Result<Vec<PathBuf>, MediaError> {
    let posts: Vec<Post> =
        serde_json::from_value(serde_json::to_value(&edition.packaging.posts).map_err(setup)?)
            .map_err(setup)?;
    retold_deliver::validate_posts(&posts).map_err(setup)?;
    let chapters = chapters(edition)?;
    let mut files = BTreeMap::<String, Vec<u8>>::new();
    for post in &posts {
        files.insert(
            format!("title.{}.txt", post.platform),
            format!("{}\n", post.title).into_bytes(),
        );
        files.insert(
            format!("description.{}.txt", post.platform),
            format!("{}\n\n{}\n", post.description, post.hashtags.join(" ")).into_bytes(),
        );
        files.insert(
            format!("tags.{}.json", post.platform),
            serde_json::to_vec_pretty(&post.hashtags).map_err(setup)?,
        );
    }
    files.insert(
        "chapters.txt".into(),
        retold_deliver::chapters_text(&chapters)
            .map_err(setup)?
            .into_bytes(),
    );
    files.insert(
        "chapters.json".into(),
        serde_json::to_vec_pretty(&chapters).map_err(setup)?,
    );
    files.insert(
        "metadata.json".into(),
        serde_json::to_vec_pretty(&posts).map_err(setup)?,
    );
    files.insert("post.md".into(),b"# Posting checklist\n\n- [ ] Review the exported video and reframing.\n- [ ] Choose one of the three thumbnail candidates using thumbnail-ranking.json.\n- [ ] Review titles, descriptions, tags and caption files.\n- [ ] Check chapter eligibility on the chosen platform (short sections may not qualify).\n- [ ] Check rights, disclosures and platform requirements.\n- [ ] Upload manually; this package never posts automatically.\n".to_vec());
    add_captions(&mut files, &edition.sidecars)?;
    let key = crate::store::hash(&(&edition.variant, directory, &files))?;
    let step = format!(
        "deliver-metadata-{}-{}",
        edition_id(&edition.variant),
        &key[..16]
    );
    let paths: BTreeMap<PathBuf, String> = crate::local_stages::run_stage_step(
        &context.media.store,
        &step,
        &serde_json::json!({"key":key}),
        &context.media.cancel,
        || async {
            let mut hashes = BTreeMap::new();
            for (name, bytes) in &files {
                let path = directory.join(name);
                if name == "post.md" {
                    if !path.exists() {
                        fsx::write_atomic(&path, bytes)?;
                    }
                } else {
                    fsx::write_atomic(&path, bytes)?;
                    hashes.insert(path, blake3::hash(bytes).to_hex().to_string());
                }
            }
            Ok(hashes)
        },
    )
    .await?;
    verify(&paths)?;
    Ok(paths.into_keys().collect())
}

pub fn caption_name(path: &Path) -> Result<String, MediaError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .ok_or_else(|| setup("caption extension missing"))?;
    if !["srt", "vtt"].contains(&ext) {
        return Err(setup("unsupported caption sidecar"));
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .ok_or_else(|| setup("caption filename missing"))
}
fn add_captions(
    files: &mut BTreeMap<String, Vec<u8>>,
    sidecars: &[PathBuf],
) -> Result<(), MediaError> {
    for path in sidecars {
        let name = caption_name(path)?;
        let bytes = std::fs::read(path).map_err(setup)?;
        if files.get(&name).is_some_and(|saved| saved != &bytes) {
            return Err(setup("caption filename collision"));
        }
        files.insert(name, bytes);
    }
    Ok(())
}

pub fn verify(files: &BTreeMap<PathBuf, String>) -> Result<(), MediaError> {
    for (path, expected) in files {
        let bytes = std::fs::read(path).map_err(setup)?;
        if blake3::hash(&bytes).to_hex().as_str() != expected {
            return Err(setup(format!("delivery file changed: {}", path.display())));
        }
    }
    Ok(())
}
fn setup(error: impl std::fmt::Display) -> MediaError {
    EngineError::Setup(error.to_string()).into()
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailReview {
    pub ranking: Vec<ThumbnailScore>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailScore {
    pub candidate: u32,
    pub score: f64,
    pub usable: bool,
    pub reason: String,
}

pub async fn rank(
    context: &Context,
    delivery: &crate::workflow::Delivery,
) -> Result<(), MediaError> {
    let images = std::iter::once(delivery.thumbnail.clone())
        .chain(delivery.thumbnail_candidates.clone())
        .collect();
    let editions_path = context
        .layout
        .stage(crate::Stage::Captions)
        .join("editions.json");
    let editions: Vec<Edition> =
        serde_json::from_slice(&std::fs::read(&editions_path).map_err(setup)?).map_err(setup)?;
    let edition = editions
        .iter()
        .find(|e| edition_id(&e.variant) == delivery.edition)
        .ok_or_else(|| setup("thumbnail story edition missing"))?;
    let story = serde_json::to_string(&edition.units).map_err(setup)?;
    let review: ThumbnailReview=context.ask(&format!("deliver-thumbnail-review-{}",delivery.id),
        context.guide.manifest().roles.image_qa,
        format!("Story title: {}. Narration (source evidence): {}. Rank these three thumbnail candidates numbered 1, 2, 3 for small-screen legibility, subject visibility, composition and faithful representation of the story. Score each 0–10 and explain visible strengths or defects. Mark usable false for clipped/illegible text or hidden subjects. Return all three exactly once, best first. These are editorial scores, never CTR predictions. The user chooses the final thumbnail.",context.meta.title,story),images).await?;
    let ids: std::collections::BTreeSet<_> = review.ranking.iter().map(|s| s.candidate).collect();
    if ids != [1, 2, 3].into_iter().collect()
        || review.ranking.len() != 3
        || review.ranking.iter().any(|s| {
            !s.score.is_finite() || !(0.0..=10.0).contains(&s.score) || s.reason.trim().is_empty()
        })
        || review.ranking.windows(2).any(|s| s[0].score < s[1].score)
        || !review.ranking.iter().any(|s| s.usable)
    {
        return Err(MediaError::Quality(
            "invalid/unusable thumbnail ranking".into(),
        ));
    }
    fsx::write_json_atomic(
        &delivery.directory.join("thumbnail-ranking.json"),
        &serde_json::json!({"rubric":"legibility, subject visibility, composition, story fidelity; not predicted CTR","ranking":review.ranking,"selected":null}),
    )?;
    Ok(())
}

pub fn inventory(directory: &Path) -> Result<BTreeMap<PathBuf, String>, MediaError> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(directory).map_err(setup)? {
        let path = entry.map_err(setup)?.path();
        if path.is_file() && path.file_name().is_some_and(|n| n != "package.json") {
            let bytes = std::fs::read(&path).map_err(setup)?;
            files.insert(path, blake3::hash(&bytes).to_hex().to_string());
        }
    }
    Ok(files)
}

pub async fn finish(
    context: &Context,
    delivery: &crate::workflow::Delivery,
) -> Result<(), MediaError> {
    for path in std::iter::once(&delivery.video)
        .chain(std::iter::once(&delivery.thumbnail))
        .chain(&delivery.thumbnail_candidates)
    {
        if !std::fs::metadata(path).is_ok_and(|m| m.len() > 0) {
            return Err(setup("incomplete delivery media"));
        }
    }
    let files = inventory(&delivery.directory)?;
    for name in [
        "chapters.json",
        "metadata.json",
        "post.md",
        "thumbnail-ranking.json",
    ] {
        if !files.contains_key(&delivery.directory.join(name)) {
            return Err(setup(format!("missing package file {name}")));
        }
    }
    let key = crate::store::hash(&files)?;
    let step = format!("deliver-package-{}", delivery.id);
    let record: BTreeMap<PathBuf, String> = crate::local_stages::run_stage_step(
        &context.media.store,
        &step,
        &serde_json::json!({"hash":key}),
        &context.media.cancel,
        || async { Ok(files.clone()) },
    )
    .await?;
    verify(&record)?;
    let relative: BTreeMap<_, _> = record
        .iter()
        .map(|(p, h)| {
            Ok((
                p.strip_prefix(&delivery.directory)
                    .map_err(setup)?
                    .to_string_lossy()
                    .into_owned(),
                h,
            ))
        })
        .collect::<Result<_, MediaError>>()?;
    fsx::write_json_atomic(
        &delivery.directory.join("package.json"),
        &serde_json::json!({"schema":1,"edition":delivery.edition,"aspect":delivery.aspect,"files":relative,"thumbnail_selection":"pending user choice"}),
    )?;
    Ok(())
}

pub fn thumbnail_inventory(
    delivery: &crate::workflow::Delivery,
) -> Result<BTreeMap<PathBuf, String>, MediaError> {
    let paths = std::iter::once(delivery.thumbnail.clone())
        .chain(delivery.thumbnail_candidates.clone())
        .chain(std::iter::once(
            delivery.directory.join("thumbnail-ranking.json"),
        ));
    paths
        .map(|path| {
            let hash = blake3::hash(&std::fs::read(&path).map_err(setup)?)
                .to_hex()
                .to_string();
            Ok((path, hash))
        })
        .collect()
}
