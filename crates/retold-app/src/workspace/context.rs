//! One resolved workspace context shared by command and terminal entry points.
use std::path::{Path, PathBuf};

use retold_engine::{Paths, paths::workspace};

use crate::protocol::{Diagnostic, ErrorCode};

#[derive(Debug, Clone)]
pub struct Context {
    pub root: PathBuf,
    pub config: PathBuf,
    pub library: PathBuf,
    pub managed: bool,
}

impl Context {
    #[must_use]
    pub fn directories(&self) -> Vec<(&'static str, PathBuf)> {
        vec![
            ("channels", self.config.join("channels")),
            ("guides", self.config.join("guides")),
            ("presets", self.config.join("presets")),
            ("series", self.library.join("series")),
            ("audio/music", self.library.join("audio/music")),
            ("audio/sfx", self.library.join("audio/sfx")),
        ]
    }

    pub fn resolve(explicit: Option<&Path>) -> Result<Self, Diagnostic> {
        if let Some(root) = explicit {
            if let Some(context) = Self::managed(root)? {
                return Ok(context);
            }
            // Explicit pre-workspace roots retain their flat portable layout.
            return Ok(Self {
                root: root.to_path_buf(),
                config: root.to_path_buf(),
                library: root.to_path_buf(),
                managed: false,
            });
        }
        let paths = Paths::discover().map_err(|error| diagnostic(&error))?;
        if let Some(context) = Self::managed(&paths.library)? {
            return Ok(context);
        }
        Ok(Self {
            root: paths.library.clone(),
            config: paths.config,
            library: paths.library,
            managed: false,
        })
    }

    fn managed(root: &Path) -> Result<Option<Self>, Diagnostic> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        for parent in root.ancestors() {
            let has_manifest = parent.join(workspace::FILENAME).exists();
            if has_manifest {
                let paths = Paths::from_workspace(parent).map_err(|error| diagnostic(&error))?;
                if parent == root || paths.library == root {
                    return Ok(Some(Self {
                        root: parent.to_path_buf(),
                        config: paths.config,
                        library: paths.library,
                        managed: true,
                    }));
                }
                break;
            }
        }
        Ok(Option::default())
    }
}

fn diagnostic(error: &retold_engine::EngineError) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &error.to_string())
}
