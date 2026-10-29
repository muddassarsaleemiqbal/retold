//! Plot sources. Season requests only accept a matching season page, so a failed lookup
//! cannot quietly turn into a recap of the entire series.

use std::collections::HashSet;
use std::fmt::Write;
use std::time::Duration;

use retold_guide::{Guide, Inputs, manifest::SourceKind, template};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::EngineError;
mod saved;
pub use saved::{read_saved_source, saved_source_hash};

const API: &str = "https://en.wikipedia.org/w/api.php";
const MAX_RESPONSE: usize = 4_000_000;
/// Whole books are larger than any API answer.
const MAX_BOOK: usize = 8_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub title: String,
    pub url: Option<String>,
    pub text: String,
    /// Each source read, when research gathers several. `text` joins them with headers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<SourcePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePart {
    /// Short citation id, `S1`, `S2`…
    pub id: String,
    pub kind: String,
    pub title: String,
    pub url: Option<String>,
    pub text: String,
}

impl Source {
    /// Joins parts into one cited text.
    #[must_use]
    pub fn from_parts(title: &str, parts: Vec<SourcePart>) -> Self {
        let text = parts
            .iter()
            .map(|p| {
                format!(
                    "## [{}] {}: {}{}\n\n{}",
                    p.id,
                    p.kind,
                    p.title,
                    p.url
                        .as_ref()
                        .map_or_else(String::new, |u| format!(" <{u}>")),
                    p.text.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        Self {
            title: title.into(),
            url: parts.first().and_then(|p| p.url.clone()),
            text,
            parts,
        }
    }
}

fn client() -> Result<reqwest::Client, EngineError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .user_agent("Retold/0.1 (source research)")
        .build()
        .map_err(network)
}

/// Reads every configured online source (web research runs later, through a model), plus
/// the user's file. Optional sources that fail or find nothing are skipped; a required one
/// stops the run.
pub async fn gather(
    guide: &Guide,
    inputs: &Inputs,
    local: Option<Source>,
    cancel: &CancellationToken,
) -> Result<Source, EngineError> {
    let research = &guide.manifest().research;
    let client = client()?;
    let title = work_title(inputs);
    let author = inputs.get("author").map(ToString::to_string);
    let mut parts: Vec<SourcePart> = Vec::new();
    let push = |parts: &mut Vec<SourcePart>, kind: &str, found: Source| {
        if found.text.trim().is_empty() {
            return;
        }
        parts.push(SourcePart {
            id: format!("S{}", parts.len() + 1),
            kind: kind.into(),
            title: found.title,
            url: found.url,
            text: found.text,
        });
    };
    if let Some(local) = local {
        push(&mut parts, "file", local);
    }
    for spec in &research.sources {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        let budget = spec.max_chars.unwrap_or(research.max_chars);
        let query = render_opt(spec.query.as_deref(), inputs)?.unwrap_or_else(|| title.clone());
        let found: Result<Vec<Source>, EngineError> = match spec.kind {
            SourceKind::File | SourceKind::Web => continue,
            SourceKind::Wikipedia => {
                let lang = render_opt(spec.language.as_deref(), inputs)?
                    .filter(|l| !l.trim().is_empty())
                    .unwrap_or_else(|| "en".into());
                wikipedia_all(&client, inputs, spec, &query, &lang, budget, cancel).await
            }
            SourceKind::Wikiquote => mediawiki_page(
                &client,
                "https://en.wikiquote.org/w/api.php",
                &query,
                &title,
                subject_kind(inputs),
                budget,
                cancel,
            )
            .await
            .map(|s| s.into_iter().collect()),
            _ => match spec.kind {
                SourceKind::Fandom => fandom(&client, inputs, &query, &title, budget, cancel)
                    .await
                    .map(|s| s.into_iter().collect()),
                SourceKind::Gutenberg => {
                    gutenberg(&client, &query, &title, author.as_deref(), budget, cancel)
                        .await
                        .map(|s| s.into_iter().collect())
                }
                _ => Ok(Vec::new()),
            },
        };
        let kind = serde_json::to_value(spec.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        match found {
            Ok(found) if !found.is_empty() => {
                for source in found {
                    push(&mut parts, &kind, source);
                }
            }
            Ok(_) | Err(_) if spec.optional => {}
            Ok(_) => {
                return Err(EngineError::Setup(format!(
                    "no {kind} source found for `{query}`; supply facts with --source-file PATH"
                )));
            }
            Err(e) => return Err(e),
        }
    }
    if parts.is_empty() {
        return Err(EngineError::Setup(
            "research found no sources; supply facts with --source-file PATH".into(),
        ));
    }
    Ok(Source::from_parts(&title, parts))
}

fn render_opt(template_text: Option<&str>, inputs: &Inputs) -> Result<Option<String>, EngineError> {
    template_text
        .map(|t| {
            template::render("research", t, json!({"inputs": inputs.template_context()}))
                .map_err(|e| EngineError::Template(e.to_string()))
        })
        .transpose()
}

/// The best-matching article plus up to `spec.related` other matching articles
/// (adaptations, remakes, sequels), each its own source.
async fn wikipedia_all(
    client: &reqwest::Client,
    inputs: &Inputs,
    spec: &retold_guide::manifest::SourceSpec,
    query: &str,
    lang: &str,
    budget: usize,
    cancel: &CancellationToken,
) -> Result<Vec<Source>, EngineError> {
    let api = format!("https://{lang}.wikipedia.org/w/api.php");
    let title = inputs
        .get("title")
        .map_or_else(|| query.to_owned(), ToString::to_string);
    let year = inputs
        .get("year")
        .and_then(|value| value.to_string().parse::<u32>().ok());
    let response = get_from(
        client,
        &api,
        &[
            ("action", "query"),
            ("list", "search"),
            ("srsearch", query),
            ("srlimit", "10"),
        ],
        cancel,
    )
    .await?;
    let candidates = page_candidates(inputs, &title, None, year, &response);
    let mut seen = HashSet::new();
    let mut out: Vec<Source> = Vec::new();
    let wanted = 1 + spec.related as usize;
    for page in candidates
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .take(10)
    {
        if out.len() >= wanted {
            break;
        }
        // Related articles must still be about this title, not just mention it.
        if !out.is_empty() && !page.to_lowercase().contains(&title.to_lowercase()) {
            continue;
        }
        let year_filter = if out.is_empty() { year } else { None };
        let sections = if spec.full {
            SectionScope::Full(&spec.sections)
        } else {
            SectionScope::PlotOnly(&spec.sections)
        };
        if let Some(source) = article_from(
            client,
            &api,
            lang,
            &page,
            None,
            year_filter,
            sections,
            budget,
            cancel,
        )
        .await?
            && out.iter().all(|s| s.title != source.title)
        {
            out.push(source);
        }
    }
    Ok(out)
}

/// One page from a `MediaWiki` site with the text-extracts API (Wikiquote).
async fn mediawiki_page(
    client: &reqwest::Client,
    api: &str,
    query: &str,
    title: &str,
    kind: SubjectKind,
    budget: usize,
    cancel: &CancellationToken,
) -> Result<Option<Source>, EngineError> {
    let search = get_from(
        client,
        api,
        &[
            ("action", "query"),
            ("list", "search"),
            ("srsearch", query),
            ("srlimit", "5"),
        ],
        cancel,
    )
    .await?;
    let Some((_, page)) = best_title(&search, title, kind) else {
        return Ok(None);
    };
    let data = get_from(
        client,
        api,
        &[
            ("action", "query"),
            ("prop", "extracts"),
            ("explaintext", "1"),
            ("titles", &page),
            ("redirects", "1"),
        ],
        cancel,
    )
    .await?;
    let text = data["query"]["pages"][0]["extract"]
        .as_str()
        .unwrap_or_default();
    if text.trim().len() < 200 {
        return Ok(None);
    }
    let base = api.trim_end_matches("/w/api.php");
    Ok(Some(Source {
        title: page.clone(),
        url: Some(format!("{base}/wiki/{}", page.replace(' ', "_"))),
        text: truncate(text, budget),
        parts: vec![],
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    Book,
    Other,
}

/// The best search hit for the work with its score: an exact title first; for books, the
/// `(novel)` page over film, series and franchise pages.
fn best_title(search: &Value, title: &str, kind: SubjectKind) -> Option<(i32, String)> {
    search["query"]["search"]
        .as_array()?
        .iter()
        .filter_map(|h| h["title"].as_str())
        .map(|hit| (title_score(hit, title, kind), hit.to_owned()))
        .filter(|(score, _)| *score > 0)
        .max_by_key(|(score, _)| *score)
}

#[must_use]
pub fn title_score(hit: &str, title: &str, kind: SubjectKind) -> i32 {
    let hit = hit.to_lowercase();
    let wanted = title.to_lowercase();
    let mut score = if hit == wanted {
        10
    } else if hit.contains(&wanted) {
        3
    } else {
        return 0;
    };
    if kind == SubjectKind::Book {
        if hit.contains("(novel)") || hit.contains("(book)") {
            score += 9;
        }
        if [
            "film",
            "miniseries",
            "tv series",
            "franchise",
            "musical",
            "opera",
            "video game",
        ]
        .iter()
        .any(|t| hit.contains(t))
        {
            score -= 6;
        }
    }
    score
}

/// The title research looks for: `title`, else the part being told (not a canon build),
/// else the whole work.
#[must_use]
pub fn work_title(inputs: &Inputs) -> String {
    ["title", "part", "work"]
        .iter()
        .filter_map(|k| inputs.get(k).map(ToString::to_string))
        .find(|t| !t.trim().is_empty() && !t.trim().eq_ignore_ascii_case(crate::canon::BUILD))
        .unwrap_or_default()
}

/// Book guides name an author and no film or series kind.
fn subject_kind(inputs: &Inputs) -> SubjectKind {
    if inputs.get("author").is_some()
        && inputs
            .get("kind")
            .is_none_or(|k| !matches!(k.to_string().as_str(), "film" | "series"))
    {
        SubjectKind::Book
    } else {
        SubjectKind::Other
    }
}

/// Fandom wikis are named after the work or its author; both are tried.
async fn fandom(
    client: &reqwest::Client,
    inputs: &Inputs,
    query: &str,
    title: &str,
    budget: usize,
    cancel: &CancellationToken,
) -> Result<Option<Source>, EngineError> {
    let slug = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
    };
    let mut wikis = vec![slug(title), slug(&title.replace('&', "and"))];
    // A franchise's wiki is named after the franchise, not the part.
    if let Some(work) = inputs.get("work").map(ToString::to_string) {
        wikis.extend([slug(&work), slug(&work.replace('&', "and"))]);
    }
    if let Some(author) = inputs.get("author") {
        wikis.push(slug(&author.to_string()));
    }
    wikis.retain(|w| !w.is_empty());
    wikis.dedup();
    let kind = subject_kind(inputs);
    let mut best: Option<(i32, String, String)> = None;
    for wiki in wikis {
        let api = format!("https://{wiki}.fandom.com/api.php");
        let Ok(search) = get_from(
            client,
            &api,
            &[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", query),
                ("srlimit", "8"),
            ],
            cancel,
        )
        .await
        else {
            continue;
        };
        if let Some((score, page)) = best_title(&search, title, kind)
            && best.as_ref().is_none_or(|(b, _, _)| score > *b)
        {
            best = Some((score, wiki, page));
        }
    }
    let Some((_, wiki, page)) = best else {
        return Ok(None);
    };
    let api = format!("https://{wiki}.fandom.com/api.php");
    let data = get_from(
        client,
        &api,
        &[
            ("action", "parse"),
            ("page", &page),
            ("prop", "wikitext"),
            ("redirects", "1"),
        ],
        cancel,
    )
    .await?;
    let text = clean_wiki_page(data["parse"]["wikitext"].as_str().unwrap_or_default());
    if text.len() >= 400 {
        return Ok(Some(Source {
            title: format!("{page} ({wiki} Fandom)"),
            url: Some(format!(
                "https://{wiki}.fandom.com/wiki/{}",
                page.replace(' ', "_")
            )),
            text: truncate(&text, budget),
            parts: vec![],
        }));
    }
    Ok(None)
}

/// Wikitext to readable lines, keeping headings and paragraphs.
fn clean_wiki_page(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.starts_with('=') {
                line.to_owned()
            } else {
                clean_wikitext(line)
            }
        })
        .filter(|line| !line.trim().is_empty() && !line.starts_with('|') && !line.starts_with('!'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The plain-text book from Project Gutenberg, without the licence header and footer.
async fn gutenberg(
    client: &reqwest::Client,
    query: &str,
    title: &str,
    author: Option<&str>,
    budget: usize,
    cancel: &CancellationToken,
) -> Result<Option<Source>, EngineError> {
    let list = fetch_bytes(
        client,
        &format!("https://gutendex.com/books/?search={}", urlencode(query)),
        MAX_RESPONSE,
        cancel,
    )
    .await?;
    let list: Value = serde_json::from_slice(&list).map_err(network)?;
    let wanted = title.to_lowercase();
    let Some(book) = list["results"]
        .as_array()
        .and_then(|books| books.iter().find(|b| gutenberg_matches(b, &wanted, author)))
    else {
        return Ok(None);
    };
    let Some(url) = book["formats"].as_object().and_then(|formats| {
        formats
            .iter()
            .find(|(mime, _)| mime.starts_with("text/plain"))
            .and_then(|(_, url)| url.as_str())
    }) else {
        return Ok(None);
    };
    let bytes = fetch_bytes(client, url, MAX_BOOK, cancel).await?;
    let text = gutenberg_body(&String::from_utf8_lossy(&bytes));
    if text.len() < 2_000 {
        return Ok(None);
    }
    Ok(Some(Source {
        title: book["title"].as_str().unwrap_or(title).to_owned(),
        url: Some(format!(
            "https://www.gutenberg.org/ebooks/{}",
            book["id"].as_u64().unwrap_or_default()
        )),
        text: truncate(&text, budget),
        parts: vec![],
    }))
}

/// A Gutenberg record is the work only when its title contains the wanted title and, when
/// an author is known, an author's name contains the author's surname.
fn gutenberg_matches(book: &Value, title: &str, author: Option<&str>) -> bool {
    let title_ok = book["title"]
        .as_str()
        .is_some_and(|t| t.to_lowercase().contains(title));
    let surname = author
        .and_then(|a| a.split_whitespace().last())
        .map(str::to_lowercase);
    let author_ok = surname.is_none_or(|surname| {
        book["authors"].as_array().is_some_and(|authors| {
            authors.iter().any(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.to_lowercase().contains(&surname))
            })
        })
    });
    title_ok && author_ok
}

/// The text between Gutenberg's `*** START OF` and `*** END OF` markers.
#[must_use]
pub fn gutenberg_body(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let start = text
        .find("*** START OF")
        .and_then(|at| text[at..].find('\n').map(|nl| at + nl + 1))
        .unwrap_or(0);
    let end = text[start..]
        .find("*** END OF")
        .map_or(text.len(), |at| start + at);
    text[start..end].trim().to_owned()
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn fetch_bytes(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, EngineError> {
    tokio::select! {
        () = cancel.cancelled() => Err(EngineError::Cancelled),
        result = async {
            let mut response = client.get(url).send().await.map_err(network)?.error_for_status().map_err(network)?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(network)? {
                if bytes.len() + chunk.len() > limit { return Err(network("response too large")); }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        } => result,
    }
}

pub async fn fetch(
    guide: &Guide,
    inputs: &Inputs,
    cancel: &CancellationToken,
) -> Result<Source, EngineError> {
    let spec = guide
        .manifest()
        .research
        .sources
        .iter()
        .find(|s| s.kind == SourceKind::Wikipedia)
        .ok_or_else(|| {
            EngineError::Setup("no Wikipedia source configured; pass --source-file".into())
        })?;
    let query = template::render(
        "research.query",
        spec.query.as_deref().unwrap_or("{{ inputs.title }}"),
        json!({"inputs": inputs.template_context()}),
    )
    .map_err(|e| EngineError::Template(e.to_string()))?;
    let season = spec
        .season_from
        .as_ref()
        .and_then(|key| inputs.get(key))
        .and_then(|v| season_number(&v.to_string()));
    let title = inputs
        .get("title")
        .map_or_else(|| query.clone(), ToString::to_string);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("Retold/0.1 (source research)")
        .build()
        .map_err(network)?;
    let search = season.map_or_else(|| query.clone(), |n| format!("\"{title}\" season {n}"));
    let response = get(
        &client,
        &[
            ("action", "query"),
            ("list", "search"),
            ("srsearch", &search),
            ("srlimit", "8"),
        ],
        cancel,
    )
    .await?;
    let year = inputs
        .get("year")
        .and_then(|value| value.to_string().parse::<u32>().ok());
    let candidates = page_candidates(inputs, &title, season, year, &response);
    let mut seen = HashSet::new();
    for page in candidates
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .take(8)
    {
        if season.is_some_and(|n| season_number(&page) != Some(n)) {
            continue;
        }
        if let Some(source) = article(
            &client,
            &page,
            season,
            year,
            &spec.sections,
            guide.manifest().research.max_chars,
            cancel,
        )
        .await?
        {
            return Ok(source);
        }
    }
    Err(EngineError::Setup(format!(
        "no matching source found for `{search}`; supply source facts with --source-file PATH"
    )))
}

/// Ranked Wikipedia page titles to try: exact disambiguated titles first, then search hits.
fn page_candidates(
    inputs: &Inputs,
    title: &str,
    season: Option<u32>,
    year: Option<u32>,
    response: &serde_json::Value,
) -> Vec<String> {
    let film = inputs
        .get("kind")
        .and_then(|v| match v.to_string().as_str() {
            "film" => Some(true),
            "series" => Some(false),
            _ => None,
        });
    let mut candidates = Vec::new();
    if let (Some(year), Some(film)) = (year, film) {
        candidates.push(format!(
            "{title} ({year} {})",
            if film { "film" } else { "TV series" }
        ));
    }
    if let Some(n) = season {
        candidates.extend([
            format!("{title} season {n}"),
            format!("{title} (season {n})"),
        ]);
    }
    if let Some(hits) = response["query"]["search"].as_array() {
        candidates.extend(
            hits.iter()
                .filter_map(|h| h["title"].as_str().map(str::to_owned)),
        );
    }
    candidates
        .sort_by_key(|page| std::cmp::Reverse(candidate_score(page, title, season, year, film)));
    candidates
}

#[derive(Debug, Clone, Copy)]
enum SectionScope<'a> {
    PlotOnly(&'a [String]),
    Full(&'a [String]),
}

async fn article(
    client: &reqwest::Client,
    page: &str,
    season: Option<u32>,
    year: Option<u32>,
    sections: &[String],
    max_chars: usize,
    cancel: &CancellationToken,
) -> Result<Option<Source>, EngineError> {
    article_from(
        client,
        API,
        "en",
        page,
        season,
        year,
        SectionScope::PlotOnly(sections),
        max_chars,
        cancel,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "split pending (Phase 0 step 3c); counted by the clean-code baseline"
)]
async fn article_from(
    client: &reqwest::Client,
    api: &str,
    lang: &str,
    page: &str,
    season: Option<u32>,
    year: Option<u32>,
    sections: SectionScope<'_>,
    max_chars: usize,
    cancel: &CancellationToken,
) -> Result<Option<Source>, EngineError> {
    let data = get_from(
        client,
        api,
        &[
            ("action", "query"),
            ("prop", "extracts"),
            ("explaintext", "1"),
            ("exsectionformat", "wiki"),
            ("titles", page),
            ("redirects", "1"),
        ],
        cancel,
    )
    .await?;
    let Some(article) = data["query"]["pages"]
        .as_array()
        .and_then(|pages| pages.first())
    else {
        return Ok(None);
    };
    let actual = article["title"].as_str().unwrap_or(page);
    if year.is_some_and(|year| has_other_year(actual, year)) {
        return Ok(None);
    }
    if season.is_some_and(|n| season_number(actual) != Some(n)) {
        return Ok(None);
    }
    let extract = article["extract"].as_str().unwrap_or_default();
    let mut body = match sections {
        SectionScope::Full(names) => all_sections(extract, names),
        SectionScope::PlotOnly(names) => plot_sections(extract, names),
    };
    // Plain extracts omit episode tables. Read ShortSummary fields on season pages.
    if season.is_some() {
        let data = get_from(
            client,
            api,
            &[
                ("action", "parse"),
                ("page", actual),
                ("prop", "wikitext"),
                ("redirects", "1"),
            ],
            cancel,
        )
        .await?;
        let episodes = episode_summaries(data["parse"]["wikitext"].as_str().unwrap_or_default());
        if !episodes.is_empty() {
            let _ = write!(body, "\n\n## Episodes\n{episodes}");
        }
    }
    if body.chars().count() >= 400 {
        let lead = extract.split("\n==").next().unwrap_or_default().trim();
        let text = format!("# {actual}\n\n{lead}\n\n{body}");
        return Ok(Some(Source {
            title: actual.into(),
            url: Some(format!(
                "https://{lang}.wikipedia.org/wiki/{}",
                actual.replace(' ', "_")
            )),
            text: truncate(&text, max_chars),
            parts: vec![],
        }));
    }
    Ok(None)
}

fn network(e: impl std::fmt::Display) -> EngineError {
    EngineError::Setup(format!(
        "Wikipedia request failed: {e}; use --source-file to work offline"
    ))
}

async fn get(
    client: &reqwest::Client,
    params: &[(&str, &str)],
    cancel: &CancellationToken,
) -> Result<Value, EngineError> {
    get_from(client, API, params, cancel).await
}

async fn get_from(
    client: &reqwest::Client,
    api: &str,
    params: &[(&str, &str)],
    cancel: &CancellationToken,
) -> Result<Value, EngineError> {
    tokio::select! {
        () = cancel.cancelled() => Err(EngineError::Cancelled),
        result = async {
            let mut response = client.get(api).query(params).query(&[("format", "json"), ("formatversion", "2"), ("maxlag", "5")])
                .send().await.map_err(network)?.error_for_status().map_err(network)?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(network)? {
                if bytes.len() + chunk.len() > MAX_RESPONSE { return Err(network("response exceeded 4 MB")); }
                bytes.extend_from_slice(&chunk);
            }
            let value: Value = serde_json::from_slice(&bytes).map_err(network)?;
            if let Some(error) = value.get("error") { return Err(network(error)); }
            Ok(value)
        } => result,
    }
}

#[must_use]
pub fn season_number(scope: &str) -> Option<u32> {
    let lower = scope.to_lowercase();
    let after = lower.split_once("season")?.1.trim_start();
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[must_use]
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.into();
    }
    let clipped: String = text.chars().take(max).collect();
    // Never slice through a Unicode scalar or end halfway through a fact sentence.
    clipped.rfind(['.', '!', '?', '।', '۔']).map_or_else(
        || clipped.clone(),
        |at| {
            let end = at + clipped[at..].chars().next().map_or(0, char::len_utf8);
            clipped[..end].into()
        },
    )
}

/// Every section of an article, with the named sections first, minus reference lists.
#[must_use]
pub fn all_sections(text: &str, first: &[String]) -> String {
    let named = plot_sections(text, first);
    let skip = [
        "references",
        "external links",
        "further reading",
        "see also",
        "notes",
        "bibliography",
        "sources",
        "citations",
    ];
    let mut keep = false;
    let mut lines = Vec::new();
    let mut in_named = false;
    for line in text.lines() {
        if line.starts_with("== ") && line.ends_with(" ==") {
            let heading = line.trim_matches('=').trim().to_lowercase();
            in_named = first.iter().any(|n| heading.contains(&n.to_lowercase()));
            keep = !in_named && !skip.iter().any(|s| heading == *s);
        }
        if keep && !in_named {
            lines.push(line);
        }
    }
    let rest = lines.join("\n");
    if named.is_empty() {
        rest
    } else {
        format!("{named}\n{rest}")
    }
}

#[must_use]
pub fn plot_sections(text: &str, names: &[String]) -> String {
    let mut keep = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.starts_with("== ") && line.ends_with(" ==") {
            let heading = line.trim_matches('=').trim().to_lowercase();
            keep = names.iter().any(|n| heading.contains(&n.to_lowercase()));
        }
        if keep {
            lines.push(line);
        }
    }
    lines.join("\n")
}

#[must_use]
pub fn episode_summaries(text: &str) -> String {
    let Ok(start) = regex::Regex::new(r"(?i)\{\{\s*Episode list") else {
        return String::new();
    };
    let Ok(summary) =
        regex::Regex::new(r"(?s)\|\s*ShortSummary\s*=\s*(.*?)(?:\n\s*\|\s*\w+\s*=|\n\}\}|$)")
    else {
        return String::new();
    };
    start
        .split(text)
        .skip(1)
        .filter_map(|block| summary.captures(block))
        .enumerate()
        .map(|(i, c)| format!("Episode {}: {}", i + 1, clean_wikitext(&c[1])))
        .collect::<Vec<_>>()
        .join("\n")
}

fn clean_wikitext(text: &str) -> String {
    let mut text = text.to_owned();
    for (pattern, replacement) in [
        (r"(?s)<ref[^>]*>.*?</ref>|<ref[^>]*/>", ""),
        (r"\[\[(?:[^|\]]*\|)?([^\]]+)\]\]", "$1"),
        (r"\{\{[^{}]*\}\}", ""),
        (r"\{\{[^{}]*\}\}", ""),
        (r"<[^>]+>|'{2,}", ""),
    ] {
        if let Ok(re) = regex::Regex::new(pattern) {
            text = re.replace_all(&text, replacement).into_owned();
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[must_use]
pub fn candidate_score(
    page: &str,
    title: &str,
    season: Option<u32>,
    year: Option<u32>,
    film: Option<bool>,
) -> i32 {
    let page = page.to_lowercase();
    let mut score = if page.contains(&title.to_lowercase()) {
        3
    } else {
        -10
    };
    if let Some(n) = season {
        score += if season_number(&page) == Some(n) {
            6
        } else {
            -6
        };
    }
    if let Some(year) = year {
        score += if page.contains(&year.to_string()) {
            6
        } else if has_other_year(&page, year) {
            -6
        } else {
            0
        };
    }
    if (film == Some(true) && page.contains("film"))
        || (film == Some(false)
            && (page.contains("tv series")
                || page.contains("season")
                || page.contains("miniseries")))
    {
        score += 2;
    }
    if ["soundtrack", "list of", "video game"]
        .iter()
        .any(|term| page.contains(term))
        || (film.is_some()
            && ["character", "novel"]
                .iter()
                .any(|term| page.contains(term)))
    {
        score -= 8;
    }
    score
}

fn has_other_year(title: &str, year: u32) -> bool {
    title
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .filter_map(|part| part.parse::<u32>().ok())
        .any(|found| (1880..=2100).contains(&found) && found != year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutenberg_keeps_only_the_book() {
        let text = "Licence\r\n*** START OF THE PROJECT GUTENBERG EBOOK X ***\r\nChapter 1\r\nIt begins.\r\n*** END OF THE PROJECT GUTENBERG EBOOK X ***\r\nMore licence";
        assert_eq!(gutenberg_body(text), "Chapter 1\nIt begins.");
        let book = json!({"title": "Frankenstein; or, the modern prometheus", "authors": [{"name": "Shelley, Mary Wollstonecraft"}]});
        assert!(gutenberg_matches(
            &book,
            "frankenstein",
            Some("Mary Shelley")
        ));
        assert!(!gutenberg_matches(
            &book,
            "frankenstein",
            Some("Colleen Hoover")
        ));
    }

    #[test]
    fn book_guides_prefer_the_novel_page_over_adaptations() {
        let search = json!({"query": {"search": [
            {"title": "The Shining (film)"},
            {"title": "The Shining (miniseries)"},
            {"title": "The Shining (novel)"},
        ]}});
        assert_eq!(
            best_title(&search, "The Shining", SubjectKind::Book).map(|(_, t)| t),
            Some("The Shining (novel)".into())
        );
        let films = json!({"query": {"search": [{"title": "The Shining (film)"}]}});
        assert!(best_title(&films, "The Shining", SubjectKind::Book).is_none());
        assert!(best_title(&films, "The Shining", SubjectKind::Other).is_some());
    }

    #[test]
    fn full_articles_put_plot_first_and_drop_reference_lists() {
        let text = "Lead\n== Background ==\nWritten in 1976.\n== Plot ==\nJack arrives.\n== References ==\n1. A book.";
        let all = all_sections(text, &["plot".into()]);
        assert!(all.starts_with("== Plot ==\nJack arrives."));
        assert!(all.contains("Written in 1976."));
        assert!(!all.contains("A book."));
    }
}
