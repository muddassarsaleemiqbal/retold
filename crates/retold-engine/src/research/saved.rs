//! The saved source participates in reviewed input versions and bounded execution reads.
use crate::{EngineError, RunLayout, Stage, fsx};

const MAX_SOURCE_BYTES: u64 = 2_000_000;

pub fn saved_source_hash(layout: &RunLayout) -> Result<String, EngineError> {
    crate::store::hash(&read_source_bytes(layout)?)
}

pub fn read_saved_source(layout: &RunLayout) -> Result<Option<String>, EngineError> {
    read_source_bytes(layout)?
        .map(|bytes| {
            String::from_utf8(bytes)
                .map_err(|_| EngineError::Setup("Saved source must be valid UTF-8.".into()))
        })
        .transpose()
}

fn read_source_bytes(layout: &RunLayout) -> Result<Option<Vec<u8>>, EngineError> {
    let path = layout.stage(Stage::Research).join("source-file.txt");
    fsx::read_bounded(&path, MAX_SOURCE_BYTES)
}
