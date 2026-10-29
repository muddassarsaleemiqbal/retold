//! Mouth and eye animation for illustrated characters: phoneme-timed mouth shapes,
//! seeded blinks, mouth-sheet slicing and sprite fitting onto a face.
#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod fit;
pub mod sheet;
pub mod typography;
pub mod viseme;

pub use typography::{TextOrientation, TypographyLayout};
pub use viseme::{Eyes, SpokenWord, Viseme};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Invalid(String),
}
