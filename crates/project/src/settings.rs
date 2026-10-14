//! The settings of a project file with their defaults: `docs/spec/project-file.md` section 3.

use std::fmt;

/// The name of the project file in a project folder.
pub const PROJECT_FILE: &str = "framesmith.yaml";

/// The composition a project renders when `composition` is not written.
pub(crate) const DEFAULT_COMPOSITION: &str = "composition.xml";
/// The folder finished files go to when `paths.renders` is not written.
const DEFAULT_RENDERS: &str = "renders";
/// The bit rate of the encoded sound, in kbit/s, when `render.audio-bitrate` is not written.
const DEFAULT_AUDIO_BITRATE: u32 = 192;
/// The name a folder with no name of its own (the root of a disk) gives its project.
const NAMELESS_FOLDER: &str = "project";

/// The most workers a render may use.
pub const MAX_WORKERS: u8 = 16;

/// A setting that is one of a few words, which the project file writes in letters.
pub(crate) trait Word: Copy + 'static {
    /// Every value, in the order the spec lists them.
    const ALL: &'static [Self];

    /// The word that is written in the project file.
    fn word(self) -> &'static str;
}

/// Defines a setting that is one of a few words: its variants, the words, and the default.
macro_rules! words {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $word:literal),+ $(,)? } default $default:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $(
                #[doc = concat!("`", $word, "`.")]
                $variant,
            )+
        }

        impl $name {
            /// The word that is written in the project file.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $word),+
                }
            }
        }

        impl Word for $name {
            const ALL: &'static [$name] = &[$($name::$variant),+];

            fn word(self) -> &'static str {
                self.as_str()
            }
        }

        impl Default for $name {
            fn default() -> $name {
                $name::$default
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

words! {
    /// Which renderer draws the frames: `render.backend`.
    Backend { Auto => "auto", Cpu => "cpu", Gpu => "gpu" } default Auto
}

words! {
    /// How hard the video encoder works: `render.quality`.
    Quality { Draft => "draft", Standard => "standard", High => "high" } default Standard
}

words! {
    /// Which H.264 encoder writes the picture: `render.encoder`.
    Encoder { Hardware => "hardware", Software => "software" } default Hardware
}

words! {
    /// The codec of the picture: `render.codec`.
    Codec { H264 => "h264", Hevc => "hevc" } default H264
}

/// How many parts of the film are rendered at once: `render.workers`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Workers {
    /// A number that suits the renderer and the machine.
    #[default]
    Auto,
    /// This many, from 1 to [`MAX_WORKERS`].
    Count(u8),
}

impl fmt::Display for Workers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Workers::Auto => f.write_str("auto"),
            Workers::Count(count) => write!(f, "{count}"),
        }
    }
}

/// The values a project file sets. A key that is not written has the default of its spec; the
/// methods give the value in use, and [`Settings::name_written`] and the like say what the file wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub(crate) name: Option<String>,
    pub(crate) composition: Option<String>,
    pub(crate) renders: Option<String>,
    pub(crate) output: Option<String>,
    pub(crate) backend: Option<Backend>,
    pub(crate) quality: Option<Quality>,
    pub(crate) encoder: Option<Encoder>,
    pub(crate) codec: Option<Codec>,
    pub(crate) workers: Option<Workers>,
    pub(crate) audio_bitrate: Option<u32>,
}

impl Settings {
    /// `name` as the file wrote it; `None` when it did not.
    pub fn name_written(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The name of the project: `name`, or `folder_name` when the file does not write one.
    pub fn name<'a>(&'a self, folder_name: &'a str) -> &'a str {
        self.name.as_deref().unwrap_or(folder_name)
    }

    /// The composition file, as a path from the project folder: `composition`, or `composition.xml`.
    pub fn composition(&self) -> &str {
        self.composition.as_deref().unwrap_or(DEFAULT_COMPOSITION)
    }

    /// The folder finished files go to: `paths.renders`, or `renders`.
    pub fn renders(&self) -> &str {
        self.renders.as_deref().unwrap_or(DEFAULT_RENDERS)
    }

    /// The finished file: `render.output`, or `<paths.renders>/<name>.mp4`.
    pub fn output(&self, folder_name: &str) -> String {
        match &self.output {
            Some(output) => output.clone(),
            None => format!("{}/{}.mp4", self.renders().trim_end_matches('/'), self.name(folder_name)),
        }
    }

    /// The renderer: `render.backend`, or `auto`.
    pub fn backend(&self) -> Backend {
        self.backend.unwrap_or_default()
    }

    /// The encoder's effort: `render.quality`, or `standard`.
    pub fn quality(&self) -> Quality {
        self.quality.unwrap_or_default()
    }

    /// The H.264 encoder: `render.encoder`, or `hardware`.
    pub fn encoder(&self) -> Encoder {
        self.encoder.unwrap_or_default()
    }

    /// The codec: `render.codec`, or `h264`.
    pub fn codec(&self) -> Codec {
        self.codec.unwrap_or_default()
    }

    /// The workers: `render.workers`, or `auto`.
    pub fn workers(&self) -> Workers {
        self.workers.unwrap_or_default()
    }

    /// The bit rate of the encoded sound in kbit/s: `render.audio-bitrate`, or 192.
    pub fn audio_bitrate(&self) -> u32 {
        self.audio_bitrate.unwrap_or(DEFAULT_AUDIO_BITRATE)
    }
}

/// The name a project folder gives its project when the file writes none: the folder's own name,
/// after the path is made absolute and followed to the real folder; `project` when it has none.
pub(crate) fn folder_name(folder: &std::path::Path) -> String {
    let real = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    real.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| NAMELESS_FOLDER.to_string())
}
