use std::path::{Path, PathBuf};

use anyhow::Result;
use image::GenericImageView;

type Rgba = Vec<u8>;

const MAX: u32 = 1920;

fn get_scale(width: u32, height: u32) -> f32 {
    if width < MAX && height < MAX {
        return 1.0;
    }
    if width > height {
        width as f32 / MAX as f32
    } else {
        height as f32 / MAX as f32
    }
}

fn resize(path: &Path) -> Result<(Rgba, u32, u32)> {
    let mut img = image::open(path)?;
    let (mut width, mut height) = img.dimensions();
    let scale = get_scale(width, height);
    if scale > 1.0 {
        log::info!(
            "image is too large,{}x{} -> {}x{}",
            width,
            height,
            width as f32 / scale,
            height as f32 / scale
        );
        width = (width as f32 / scale) as u32;
        height = (height as f32 / scale) as u32;

        img = img.resize(width, height, image::imageops::FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8().into_vec();

    Ok((rgba, width, height))
}

fn compress(rgba: Rgba, width: u32, height: u32) -> Result<Vec<u8>> {
    let encoder = webp::Encoder::from_rgba(rgba.as_slice(), width, height);
    let webp = encoder.encode(85.0);
    let webp = webp.to_vec();
    Ok(webp)
}

pub fn resize_and_compress(path: &Path) -> Result<PathBuf> {
    let (rgba, width, height) = resize(path)?;
    let webp = compress(rgba, width, height)?;
    let mut new_path = path.to_path_buf();
    new_path.set_extension("webp");
    std::fs::write(&new_path, webp)?;
    std::fs::remove_file(path)?;
    Ok(new_path)
}
