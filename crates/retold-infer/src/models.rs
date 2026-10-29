//! Resumable downloads, integrity checks and atomic activation under an OS installation lock.
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub path: PathBuf,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub files: Vec<Asset>,
}
impl Manifest {
    pub fn validate(&self) -> Result<(), Error> {
        let safe =
            |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if !safe(&self.name)
            || !safe(&self.version)
            || self.license.trim().is_empty()
            || self.files.is_empty()
        {
            return Err(Error::Invalid(
                "invalid model identity or missing licence/assets".into(),
            ));
        }
        let mut paths = std::collections::BTreeSet::new();
        for asset in &self.files {
            if asset.path.as_os_str().is_empty()
                || !asset
                    .path
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
                || !paths.insert(&asset.path)
                || asset.sha256.len() != 64
                || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || asset.size == 0
                || !(asset.url.starts_with("https://")
                    || asset.url.starts_with("http://127.0.0.1:"))
            {
                return Err(Error::Invalid(
                    "invalid model asset path, URL, size or checksum".into(),
                ));
            }
        }
        Ok(())
    }
}
pub fn catalog() -> Result<Vec<Manifest>, Error> {
    Ok(serde_json::from_str(include_str!("catalog.json"))?)
}

#[derive(Debug, Clone)]
pub struct ModelStore {
    pub root: PathBuf,
}
impl ModelStore {
    pub fn discover() -> Result<Self, Error> {
        let root = std::env::var_os("RETOLD_MODELS_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                directories::ProjectDirs::from("", "", "retold")
                    .map(|p| p.data_local_dir().join("models"))
            })
            .ok_or_else(|| {
                Error::Invalid("cannot find model directory; set RETOLD_MODELS_DIR".into())
            })?;
        Ok(Self { root })
    }
    pub fn manifest(&self, name: &str) -> Result<Manifest, Error> {
        catalog()?
            .into_iter()
            .find(|m| m.name == name)
            .ok_or_else(|| {
                Error::Invalid(format!("unknown model {name}; run `retold models list`"))
            })
    }
    #[must_use]
    pub fn directory(&self, manifest: &Manifest) -> PathBuf {
        self.root.join(&manifest.name).join(&manifest.version)
    }
    pub fn verify(&self, manifest: &Manifest) -> Result<PathBuf, Error> {
        let Manifest {
            name,
            version,
            files,
            ..
        } = manifest;
        manifest.validate()?;
        let dir = self.root.join(name).join(version);
        let active_file = self.root.join(name).join("active.json");
        let installed: Manifest =
            serde_json::from_slice(&std::fs::read(&active_file).map_err(|e| {
                let why = if e.kind() == std::io::ErrorKind::NotFound {
                    "not installed".to_owned()
                } else {
                    format!("unreadable: {e}")
                };
                Error::Invalid(format!(
                    "{name} is {why}; run `retold models install {name}`"
                ))
            })?)?;
        if serde_json::to_vec(&installed)? != serde_json::to_vec(manifest)? {
            return Err(Error::Invalid(format!(
                "{name} manifest changed; run `retold models install {name}`"
            )));
        }
        for asset in files {
            verify_asset(&dir.join(&asset.path), asset).map_err(|e| {
                Error::Invalid(format!("{name}: {e}; run `retold models install {name}`"))
            })?;
        }
        if let Some(worker) = WorkerFiles::of(name)
            && (!dir.join("worker-env/bin/python").is_file()
                || std::fs::read(dir.join(worker.script.0)).ok().as_deref()
                    != Some(worker.script.1.as_bytes())
                || std::fs::read(dir.join(worker.requirements.0))
                    .ok()
                    .as_deref()
                    != Some(worker.requirements.1.as_bytes()))
        {
            return Err(Error::Invalid(format!(
                "{name} worker missing; run `retold models install {name}`"
            )));
        }
        if !self.root.exists() {
            return Err(Error::Invalid("model root missing".into()));
        }
        Ok(dir)
    }
    fn lock(&self, manifest: &Manifest) -> Result<std::fs::File, Error> {
        manifest.validate()?;
        let dir = self.root.join(&manifest.name);
        std::fs::create_dir_all(&dir)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("install.lock"))?;
        file.try_lock().map_err(|e| {
            Error::Invalid(format!(
                "{} install/remove already in progress: {e}",
                manifest.name
            ))
        })?;
        Ok(file)
    }
    pub fn remove(&self, manifest: &Manifest) -> Result<(), Error> {
        let _lock = self.lock(manifest)?;
        let active = self.root.join(&manifest.name).join("active.json");
        if active.exists() {
            std::fs::remove_file(active)?;
        }
        let dir = self.directory(manifest);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
    pub async fn install(
        &self,
        manifest: &Manifest,
        cancel: &CancellationToken,
    ) -> Result<PathBuf, Error> {
        let _lock = self.lock(manifest)?;
        let dir = self.directory(manifest);
        std::fs::create_dir_all(&dir)?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(3600))
            .build()?;
        for asset in &manifest.files {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let path = dir.join(&asset.path);
            if verify_asset(&path, asset).is_ok() {
                continue;
            }
            // An install can resume a partial file, but never considers it usable.
            let partial = path.with_extension("download");
            std::fs::create_dir_all(
                path.parent()
                    .ok_or_else(|| Error::Invalid("missing model parent".into()))?,
            )?;
            if std::fs::metadata(&partial).is_ok_and(|m| m.len() >= asset.size) {
                std::fs::remove_file(&partial)?;
            }
            download(&client, asset, &partial, cancel).await?;
            verify_asset(&partial, asset).inspect_err(|_| {
                let _ = std::fs::remove_file(&partial);
            })?;
            std::fs::rename(&partial, &path)?;
        }
        if let Some(worker) = WorkerFiles::of(&manifest.name) {
            install_worker(&dir, &worker, cancel).await?;
        }
        let parent = self.root.join(&manifest.name);
        let mut file = tempfile::NamedTempFile::new_in(&parent)?;
        file.write_all(&serde_json::to_vec_pretty(manifest)?)?;
        file.as_file().sync_all()?;
        file.persist(parent.join("active.json"))
            .map_err(|e| e.error)?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(dir)
    }
}
pub fn verify_asset(path: &Path, asset: &Asset) -> Result<(), Error> {
    let mut file = std::fs::File::open(path)?;
    if file.metadata()?.len() != asset.size {
        return Err(Error::Invalid(format!("{} size mismatch", path.display())));
    }
    let mut hash = Sha256::new();
    let mut buf = vec![0_u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if format!("{:x}", hash.finalize()) != asset.sha256 {
        return Err(Error::Invalid(format!(
            "{} SHA-256 mismatch",
            path.display()
        )));
    }
    Ok(())
}
async fn download(
    client: &reqwest::Client,
    asset: &Asset,
    partial: &Path,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    let offset = std::fs::metadata(partial).map_or(0, |m| m.len());
    let mut req = client
        .get(&asset.url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    // Gated Hugging Face repositories need the user's own token; ungated
    // mirrors need nothing.
    if asset.url.starts_with("https://huggingface.co/")
        && let Some(token) = hf_token()
    {
        req = req.bearer_auth(token);
    }
    if offset > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }
    let mut response = tokio::select! { r = req.send() => r?.error_for_status()?, () = cancel.cancelled() => return Err(Error::Cancelled) };
    let append = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if append {
        let prefix = format!("bytes {offset}-");
        if !response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with(&prefix) && v.ends_with(&format!("/{}", asset.size)))
        {
            return Err(Error::Invalid(
                "model server returned an invalid Content-Range".into(),
            ));
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(partial)?;
    let mut length = if append { offset } else { 0 };
    loop {
        let chunk = tokio::select! { r = response.chunk() => r?, () = cancel.cancelled() => { file.sync_all()?; return Err(Error::Cancelled); } };
        let Some(chunk) = chunk else { break };
        length += u64::try_from(chunk.len()).map_err(|e| Error::Invalid(e.to_string()))?;
        if length > asset.size {
            return Err(Error::Invalid("model download exceeds pinned size".into()));
        }
        file.write_all(&chunk)?;
    }
    file.sync_all()?;
    Ok(())
}
/// A managed Python worker: its script and hash-locked requirements, each as
/// (installed file name, embedded contents).
#[derive(Debug, Clone, Copy)]
pub struct WorkerFiles {
    pub script: (&'static str, &'static str),
    pub requirements: (&'static str, &'static str),
}
impl WorkerFiles {
    #[must_use]
    pub fn of(model: &str) -> Option<Self> {
        match model {
            "voice" => Some(Self {
                script: ("worker.py", include_str!("worker.py")),
                requirements: (
                    "worker-requirements.txt",
                    include_str!("worker-requirements.txt"),
                ),
            }),
            "phonemes" => Some(Self {
                script: ("phoneme_worker.py", include_str!("phoneme_worker.py")),
                requirements: (
                    "phoneme-requirements.txt",
                    include_str!("phoneme-requirements.txt"),
                ),
            }),
            "voice-indic" => Some(Self {
                script: ("indic_worker.py", include_str!("indic_worker.py")),
                requirements: (
                    "indic-requirements.txt",
                    include_str!("indic-requirements.txt"),
                ),
            }),
            _ => None,
        }
    }
}
async fn install_worker(
    dir: &Path,
    worker: &WorkerFiles,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    let uv = std::env::var_os("RETOLD_UV").unwrap_or_else(|| "uv".into());
    let uv = PathBuf::from(uv);
    let env = dir.join("worker-env");
    let python = env.join("bin/python");
    let requirements = dir.join(worker.requirements.0);
    std::fs::write(&requirements, worker.requirements.1)?;
    for args in [
        vec![
            "venv".into(),
            "--allow-existing".into(),
            "--python".into(),
            "3.11".into(),
            env.display().to_string(),
        ],
        vec![
            "pip".into(),
            "install".into(),
            "--python".into(),
            python.display().to_string(),
            "--require-hashes".into(),
            "-r".into(),
            requirements.display().to_string(),
        ],
    ] {
        let result = retold_providers::process::run(
            &uv,
            &args,
            "",
            dir,
            &retold_providers::process::Limits::default(),
            cancel,
        )
        .await?;
        if result.status != Some(0) {
            return Err(Error::Invalid(format!(
                "voice environment setup failed: {}; install uv, then reinstall the voice model",
                result.stderr
            )));
        }
    }
    std::fs::write(dir.join(worker.script.0), worker.script.1)?;
    Ok(())
}

fn hf_token() -> Option<String> {
    std::env::var("HF_TOKEN")
        .ok()
        .or_else(|| {
            let home = std::env::var_os("HF_HOME").map(PathBuf::from).or_else(|| {
                directories::BaseDirs::new().map(|b| b.home_dir().join(".cache/huggingface"))
            })?;
            std::fs::read_to_string(home.join("token")).ok()
        })
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}
pub fn file_sha256(path: &Path) -> Result<String, Error> {
    let bytes = std::fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}
