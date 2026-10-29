//! Real local depth inference and deterministic, quality-gated parallax intermediates.
#![deny(clippy::print_stdout, clippy::print_stderr)]
pub mod camera;
pub mod depth;
pub mod dibr;
pub mod inpaint;
pub mod render;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Infer(#[from] retold_infer::Error),
    #[error(transparent)]
    Render(#[from] retold_render::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("cancelled")]
    Cancelled,
}

/// Provider assets may use .png names for JPEG/WebP bytes; decode their actual format.
pub fn load_image(path: &std::path::Path) -> Result<image::DynamicImage, Error> {
    Ok(image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?)
}
