//! Offline review page. Data is inert JSON and DOM text; no provider content is inserted
//! as HTML, and there is no browser action that can approve or start video export.

use std::path::Path;

use serde_json::json;

use crate::assets::{Images, Storyboard};
use crate::context::Context;
use crate::languages::{Edition, edition_id};
use crate::media_runtime::MediaError;
use crate::{EngineError, Stage, fsx};

const TEMPLATE: &str = include_str!("../templates/review.html");

pub fn characters(
    context: &Context,
    sheets: &std::collections::BTreeMap<String, crate::media_runtime::Asset>,
) -> Result<(), MediaError> {
    use std::fmt::Write;
    let mut cards = String::new();
    for (name, asset) in sheets {
        let _ = write!(
            cards,
            "<figure><img src=\"{}\" alt=\"{} character design\"><figcaption>{}</figcaption></figure>",
            escape(&page_url(context.layout.video_dir(), &asset.path)),
            escape(name),
            escape(name)
        );
    }
    let html = format!(
        "<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>Character review</title><style>body{{font:18px/1.6 'Helvetica Neue',Arial,sans-serif;color:#182431;background:#f4f7fa;margin:0;padding:24px}}main{{max-width:1000px;margin:auto}}section{{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:24px}}figure{{margin:0}}img{{width:100%;max-height:560px;object-fit:contain}}code{{overflow-wrap:anywhere}}</style><main><h1>{}</h1><p>Check these original character designs before scene images are made. To accept them: <code>retold video approve {reference} characters</code>, then plan and continue the video again. Video permission remains separate.</p><section>{cards}</section></main></html>",
        escape(&context.meta.title),
        reference = escape(&shell_word(&reference(context)))
    );
    fsx::write_atomic(&context.layout.review_page(), html.as_bytes())?;
    Ok(())
}

/// How commands name this video: its channel, then its folder in the library.
#[must_use]
pub fn reference(context: &Context) -> String {
    format!("{}/{}", context.meta.channel, context.layout.display_path())
}

/// `word` quoted for a shell when it needs it.
fn shell_word(word: &str) -> String {
    let plain = word
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn write(
    context: &Context,
    board: &Storyboard,
    images: &Images,
    editions: &[Edition],
) -> Result<(), MediaError> {
    let source_path = context.layout.stage(Stage::Research).join("source.json");
    let source: crate::research::Source = serde_json::from_slice(
        &std::fs::read(&source_path).map_err(|e| EngineError::io(&source_path, e))?,
    )
    .map_err(|e| EngineError::Setup(e.to_string()))?;
    let frames: Vec<_> = images.frames.iter().map(|f| {
        let shot = &board.direction.shots[f.unit];
        json!({"unit":f.unit,"image":page_url(context.layout.video_dir(), &f.image.path),"action":shot.action,"shot":shot.shot,"location":shot.location,
            "style":f.qa.style_score,"character":f.qa.character_score,"attention":!f.qa.passes(),"regenerated":f.regenerated})
    }).collect();
    let editions: Vec<_> = editions.iter().map(|e|json!({"id":edition_id(&e.variant),"name":e.variant.to_string(),"language":e.variant.narration.language(),"audio":page_url(context.layout.video_dir(), &e.audio.path),
        "units":e.units.iter().zip(&e.alignment.units).map(|(u,t)|json!({"id":u.id,"section":u.section,"text":u.text,"start":t.start,"end":t.end})).collect::<Vec<_>>()})).collect();
    let receipts: Vec<_> = context
        .media
        .ledger
        .entries()?
        .into_iter()
        .map(|(id, c)| json!({"label":id,"cost":c.amount().to_string()}))
        .collect();
    let command = format!(
        "retold resume {} --allow-paid --allow-video",
        shell_word(context.layout.slug().as_str())
    );
    let data = json!({"title":context.meta.title,"source":source,"frames":frames,"editions":editions,"cost":context.media.ledger.total()?.to_string(),"receipts":receipts,"command":command});
    let encoded = serde_json::to_string(&data)
        .map_err(|e| EngineError::Setup(e.to_string()))?
        .replace('<', "\\u003c")
        .replace('&', "\\u0026");
    fsx::write_atomic(
        &context.layout.review_page(),
        TEMPLATE.replace("__REVIEW_DATA__", &encoded).as_bytes(),
    )?;
    Ok(())
}

/// The link the review page (which sits in the video folder) uses for `path`: relative inside
/// the folder, so the page keeps working when the folder is moved or served; `file://` outside.
#[must_use]
pub fn page_url(video_dir: &Path, path: &Path) -> String {
    path.strip_prefix(video_dir).map_or_else(
        |_| file_url(path),
        |relative| percent_encode(&relative.to_string_lossy()),
    )
}

#[must_use]
pub fn file_url(path: &Path) -> String {
    format!("file://{}", percent_encode(&path.to_string_lossy()))
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/:-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub fn images_only(
    context: &Context,
    board: &Storyboard,
    images: &Images,
) -> Result<(), MediaError> {
    use std::fmt::Write;
    let mut cards = String::new();
    for frame in &images.frames {
        let unit = &board.units[frame.unit];
        let _ = write!(
            cards,
            "<article><img src=\"{}\" alt=\"{}\"><div><h2>Scene {}</h2><p>{}</p><p>Style {}/10 · character {}/10{}</p></div></article>",
            escape(&page_url(context.layout.video_dir(), &frame.image.path)),
            escape(&board.direction.shots[frame.unit].action),
            frame.unit + 1,
            escape(&unit.narration),
            frame.qa.style_score,
            frame.qa.character_score,
            if frame.regenerated {
                " · regenerated"
            } else {
                ""
            }
        );
    }
    for frame in images.frames.iter().filter(|f| !f.qa.passes()) {
        let _ = write!(
            cards,
            "<p><strong>Scene {} needs attention:</strong> {}</p>",
            frame.unit + 1,
            escape(&frame.qa.problems.join("; "))
        );
    }
    for frame in images
        .frames
        .iter()
        .filter(|f| f.qa.passes() && !f.qa.notes.is_empty())
    {
        let _ = write!(
            cards,
            "<p>Scene {} notes: {}</p>",
            frame.unit + 1,
            escape(&frame.qa.notes.join("; "))
        );
    }
    let intro = if images.frames.iter().all(|f| f.qa.passes()) {
        "Image checks passed. Narration and recorded timings have not been prepared."
    } else {
        "Some frames still need attention. Narration and recorded timings have not been prepared."
    };
    let html = format!(
        "<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>Image review</title><style>body{{font:18px/1.6 'Helvetica Neue',Arial,sans-serif;color:#182431;background:#f4f7fa;margin:0;padding:24px}}main{{max-width:1000px;margin:auto}}article{{display:grid;grid-template-columns:minmax(100px,210px) minmax(0,1fr);gap:24px;border-bottom:1px solid #ccd7e0;padding:24px 0}}img{{width:100%}}p{{overflow-wrap:anywhere}}@media(max-width:500px){{article{{grid-template-columns:110px minmax(0,1fr);gap:16px}}}}</style><main><h1>{}</h1><p>{intro}</p>{cards}</main></html>",
        escape(&context.meta.title)
    );
    fsx::write_atomic(&context.layout.review_page(), html.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{page_url, shell_word};
    use std::path::Path;

    #[test]
    fn page_links_are_relative_inside_the_video_folder() {
        let video = Path::new("/lib/films/nosferatu");
        assert_eq!(
            page_url(
                video,
                Path::new("/lib/films/nosferatu/work/05-images/unit 1.png")
            ),
            "work/05-images/unit%201.png"
        );
        assert_eq!(
            page_url(video, Path::new("/elsewhere/a.png")),
            "file:///elsewhere/a.png"
        );
    }

    #[test]
    fn commands_quote_only_when_needed() {
        assert_eq!(shell_word("nosferatu-1922"), "nosferatu-1922");
        assert_eq!(shell_word("it's"), "'it'\\''s'");
    }
}
