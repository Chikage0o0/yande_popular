use std::path::{Path, PathBuf};

use anyhow::Result;
use image_compressor::{
    compressor::{Compressor, ResizeType},
    Factor,
};

const LIMIT: usize = 1920;

pub fn resize_and_compress(path: &Path) -> Result<PathBuf> {
    let source = path;
    let dest = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dest)?;
    let mut comp = Compressor::new(source, &dest);
    comp.set_factor(Factor::new_with_resize_type(
        85.0,
        ResizeType::LongestSidePixels(LIMIT),
    ));
    comp.set_delete_source(false);
    comp.set_overwrite_dest(true);
    let path = comp.compress_to_jpg().unwrap_or_else(|e| {
        log::error!("compress failed: {}", e);
        source.to_path_buf()
    });
    Ok(path)
}

#[cfg(test)]
mod test {
    use std::env;

    use super::*;
    #[test]
    fn test_resize_and_compress() -> Result<()> {
        env::set_var("RUST_LOG", "debug");
        env_logger::init();
        let path = Path::new(r"D:\Project\yande_popular\data\tmp\Term.png");
        let path = resize_and_compress(path)?;
        log::debug!("path: {:?}", path);
        Ok(())
    }
}
