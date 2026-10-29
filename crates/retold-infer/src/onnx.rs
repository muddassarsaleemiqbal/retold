//! In-process ONNX sessions, cached by model content and execution policy.
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ort::session::{RunOptions, Session};
use ort::value::Tensor;
use tokio_util::sync::CancellationToken;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Backend {
    Cpu,
    CoreMl,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Input {
    pub name: String,
    pub shape: Vec<i64>,
    #[serde(flatten)]
    pub data: TensorData,
}
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "dtype", content = "values", rename_all = "snake_case")]
pub enum TensorData {
    F32(Vec<f32>),
    I64(Vec<i64>),
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Output {
    pub name: String,
    pub shape: Vec<i64>,
    pub values: Vec<f32>,
}
type SharedSession = Arc<Mutex<Session>>;
#[derive(Debug, Default, Clone)]
pub struct OnnxSession {
    sessions: Arc<Mutex<BTreeMap<(String, Backend), SharedSession>>>,
}
const fn ort_error(e: ort::Error) -> Error {
    Error::Onnx(e)
}
impl OnnxSession {
    /// The caller verifies the model manifest first; hashing also detects changed files in-session.
    pub async fn run(
        &self,
        model: &Path,
        backend: Backend,
        inputs: Vec<Input>,
        cancel: &CancellationToken,
    ) -> Result<Vec<Output>, Error> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let model = model.to_owned();
        let sessions = Arc::clone(&self.sessions);
        let options = Arc::new(RunOptions::new().map_err(ort_error)?);
        let run_options = Arc::clone(&options);
        let token = cancel.clone();
        let mut work = tokio::task::spawn_blocking(move || {
            if token.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let key = (super::models::file_sha256(&model)?, backend);
            let session = {
                let mut cache = sessions.lock().map_err(|e| Error::Invalid(e.to_string()))?;
                if let Some(session) = cache.get(&key) {
                    Arc::clone(session)
                } else {
                    let mut builder = Session::builder().map_err(ort_error)?;
                    if backend == Backend::CoreMl {
                        builder = builder
                            .with_execution_providers([
                                ort::execution_providers::CoreMLExecutionProvider::default()
                                    .build(),
                            ])
                            .map_err(ort_error)?;
                    }
                    let session = match builder.commit_from_file(&model) {
                        Ok(session) => session,
                        Err(_) if backend == Backend::CoreMl => Session::builder()
                            .map_err(ort_error)?
                            .commit_from_file(&model)
                            .map_err(ort_error)?,
                        Err(e) => return Err(ort_error(e)),
                    };
                    let session = Arc::new(Mutex::new(session));
                    cache.insert(key, Arc::clone(&session));
                    session
                }
            };
            if token.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let tensors = inputs
                .into_iter()
                .map(|input| {
                    let tensor = match input.data {
                        TensorData::F32(values) => Tensor::from_array((input.shape, values))
                            .map(ort::value::Value::into_dyn),
                        TensorData::I64(values) => Tensor::from_array((input.shape, values))
                            .map(ort::value::Value::into_dyn),
                    }
                    .map_err(ort_error)?;
                    Ok::<_, Error>((input.name, tensor))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut session = session.lock().map_err(|e| Error::Invalid(e.to_string()))?;
            if token.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let outputs = session
                .run_with_options(tensors, &run_options)
                .map_err(ort_error)?;
            outputs
                .iter()
                .filter(|(_, value)| value.try_extract_tensor::<f32>().is_ok())
                .map(|(name, value)| {
                    let (shape, values) = value.try_extract_tensor::<f32>().map_err(ort_error)?;
                    Ok(Output {
                        name: name.into(),
                        shape: shape.to_vec(),
                        values: values.to_vec(),
                    })
                })
                .collect()
        });
        tokio::select! {
            result = &mut work => result.map_err(|e|Error::Invalid(e.to_string()))?,
            () = cancel.cancelled() => { options.terminate().map_err(ort_error)?; Err(Error::Cancelled) }
        }
    }
}
