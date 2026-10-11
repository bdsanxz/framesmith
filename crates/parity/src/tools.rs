//! Finding the user's own `ffmpeg` and `ffprobe`.

use std::path::PathBuf;

/// The two programs every comparison needs.
#[derive(Clone, Debug)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Tools {
    pub fn find() -> Result<Tools, String> {
        let find = |name: &str| measure::tool(name).ok_or_else(|| format!("{name} was not found on the search path"));
        Ok(Tools {
            ffmpeg: find("ffmpeg")?,
            ffprobe: find("ffprobe")?,
        })
    }
}
