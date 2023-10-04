#[cfg(feature = "matrix")]
mod matrix;
#[cfg(feature = "voce")]
mod voce;

#[cfg(feature = "matrix")]
pub use matrix::send_attachment;
#[cfg(feature = "voce")]
pub use voce::send_attachment;
