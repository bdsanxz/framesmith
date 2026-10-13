//! The project file, `framesmith.yaml`: the settings that belong to a project and not to one
//! composition. See `docs/spec/project-file.md`.

use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "framesmith.yaml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    Auto,
    Cpu,
    Gpu,
}

impl Renderer {
    pub fn from_name(name: &str) -> Option<Renderer> {
        match name {
            "auto" => Some(Renderer::Auto),
            "cpu" => Some(Renderer::Cpu),
            "gpu" => Some(Renderer::Gpu),
            _ => None,
        }
    }
}

/// Which H.264 encoder writes the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coder {
    /// The system's H.264 encoder run on the processor, at an average bit rate. It keeps less
    /// of the picture than the hardware does, and is for machines whose hardware does not
    /// encode the film's size.
    Software,
    /// The machine's own video encoder, at a quality setting.
    Hardware,
}

impl Coder {
    pub fn from_name(name: &str) -> Option<Coder> {
        match name {
            "software" => Some(Coder::Software),
            "hardware" => Some(Coder::Hardware),
            _ => None,
        }
    }
}

/// How many parts of a film are rendered at the same time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workers {
    /// As many as suit the renderer and the machine.
    Auto,
    Count(u32),
}

impl Workers {
    /// The most workers a render may be given.
    pub const MOST: u32 = 16;

    pub fn from_name(name: &str) -> Option<Workers> {
        match name {
            "auto" => Some(Workers::Auto),
            _ => name
                .parse()
                .ok()
                .filter(|count| (1..=Workers::MOST).contains(count))
                .map(Workers::Count),
        }
    }

    /// The number of workers for a render with `renderer`.
    pub fn count(self, renderer: Renderer) -> u32 {
        match self {
            Workers::Count(count) => count,
            Workers::Auto => {
                let cores = std::thread::available_parallelism().map_or(1, |cores| cores.get() as u32);
                match renderer {
                    // A CPU worker draws on one core, so several of them use what one leaves.
                    Renderer::Cpu => (cores / 3).clamp(1, 4),
                    // One GPU worker already keeps the card busy; more were measured to add
                    // next to nothing.
                    Renderer::Gpu | Renderer::Auto => 1,
                }
            }
        }
    }
}

/// The codec of the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    /// Plays everywhere. Hardware encodes it up to a size of its own.
    H264,
    /// For films larger than hardware encodes as H.264, 8K among them.
    Hevc,
}

impl Codec {
    pub fn from_name(name: &str) -> Option<Codec> {
        match name {
            "h264" => Some(Codec::H264),
            "hevc" => Some(Codec::Hevc),
            _ => None,
        }
    }
}

/// How hard the video encoder works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Draft,
    Standard,
    High,
}

impl Quality {
    pub fn from_name(name: &str) -> Option<Quality> {
        match name {
            "draft" => Some(Quality::Draft),
            "standard" => Some(Quality::Standard),
            "high" => Some(Quality::High),
            _ => None,
        }
    }

    /// The bits the software encoder is given for each pixel of each frame, on average.
    pub fn bits_a_pixel(self) -> f64 {
        match self {
            Quality::Draft => 0.06,
            Quality::Standard => 0.13,
            Quality::High => 0.20,
        }
    }

    /// The quality setting of a hardware encoder, on its scale of 1 to 100 (higher keeps more).
    pub fn hardware(self) -> u8 {
        match self {
            Quality::Draft => 45,
            Quality::Standard => 65,
            Quality::High => 80,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    /// The project folder: what every path of the project is relative to.
    pub folder: PathBuf,
    pub name: String,
    /// The composition `render` draws, relative to the folder.
    pub composition: PathBuf,
    /// Where finished files go, relative to the folder.
    pub renders: PathBuf,
    /// The finished file, relative to the folder, when the file names one.
    pub output: Option<PathBuf>,
    pub renderer: Renderer,
    pub quality: Quality,
    pub coder: Coder,
    pub codec: Codec,
    pub workers: Workers,
    /// Of the encoded sound, in kbit/s.
    pub audio_bitrate: u32,
}

/// One thing wrong with the file, with the line it is on.
#[derive(Debug, PartialEq)]
pub struct Problem {
    pub line: usize,
    pub message: String,
}

impl Project {
    /// The project a folder is without a project file.
    pub fn defaults(folder: &Path) -> Project {
        let named = folder
            .canonicalize()
            .ok()
            .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()));
        Project {
            folder: folder.to_path_buf(),
            name: named.filter(|name| !name.is_empty()).unwrap_or_else(|| "video".to_string()),
            composition: PathBuf::from("composition.xml"),
            renders: PathBuf::from("renders"),
            output: None,
            renderer: Renderer::Auto,
            quality: Quality::Standard,
            coder: Coder::Hardware,
            codec: Codec::H264,
            workers: Workers::Auto,
            audio_bitrate: 192,
        }
    }

    /// The finished file: the one the project file names, or `<renders>/<name>.mp4`.
    pub fn output_path(&self) -> PathBuf {
        let relative = self
            .output
            .clone()
            .unwrap_or_else(|| self.renders.join(format!("{}.mp4", self.name)));
        self.folder.join(relative)
    }
}

struct Reader {
    problems: Vec<Problem>,
}

impl Reader {
    fn problem(&mut self, node: &MarkedYaml, message: String) {
        self.problems.push(Problem {
            line: node.span.start.line(),
            message,
        });
    }

    fn text(&mut self, key: &str, node: &MarkedYaml) -> Option<String> {
        match &node.data {
            YamlData::Value(Scalar::String(text)) if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => {
                self.problem(node, format!("`{key}` must be text"));
                None
            }
        }
    }

    /// A path inside the project folder.
    fn path(&mut self, key: &str, node: &MarkedYaml) -> Option<PathBuf> {
        let text = self.text(key, node)?;
        let path = PathBuf::from(&text);
        let inside = path.is_relative() && !path.components().any(|part| matches!(part, std::path::Component::ParentDir));
        if !inside {
            self.problem(node, format!("`{key}: {text}` must be a path inside the project folder"));
            return None;
        }
        Some(path)
    }

    /// The entries of a mapping, each key as text. Anything that is not a mapping is a problem.
    fn entries<'a>(&mut self, what: &str, node: &'a MarkedYaml<'a>) -> Vec<(String, &'a MarkedYaml<'a>, &'a MarkedYaml<'a>)> {
        match &node.data {
            YamlData::Value(Scalar::Null) => Vec::new(),
            YamlData::Mapping(mapping) => {
                let mut entries = Vec::new();
                for (key, value) in mapping {
                    match &key.data {
                        YamlData::Value(Scalar::String(name)) => entries.push((name.to_string(), key, value)),
                        _ => self.problem(key, format!("a key of {what} must be a plain word")),
                    }
                }
                entries
            }
            _ => {
                self.problem(node, format!("{what} must be a list of `key: value` lines"));
                Vec::new()
            }
        }
    }
}

/// The project a file describes, or everything that is wrong with the file.
pub fn parse(source: &str, folder: &Path) -> Result<Project, Vec<Problem>> {
    let documents = MarkedYaml::load_from_str(source).map_err(|error| {
        vec![Problem {
            line: error.marker().line(),
            message: format!("the file is not well-formed YAML: {}", error.info()),
        }]
    })?;
    let mut project = Project::defaults(folder);
    let mut reader = Reader { problems: Vec::new() };
    if documents.len() > 1 {
        reader.problem(&documents[1], "the file must hold one document".to_string());
    }
    let Some(root) = documents.first() else { return Ok(project) };
    for (key, at, value) in reader.entries("the project file", root) {
        match key.as_str() {
            "name" => {
                if let Some(name) = reader.text("name", value) {
                    if name.contains(['/', '\\']) {
                        reader.problem(value, format!("`name: {name}` must not hold a path separator"));
                    } else {
                        project.name = name;
                    }
                }
            }
            "composition" => project.composition = reader.path("composition", value).unwrap_or(project.composition),
            "paths" => {
                for (key, at, value) in reader.entries("`paths`", value) {
                    match key.as_str() {
                        "renders" => project.renders = reader.path("paths.renders", value).unwrap_or(project.renders),
                        other => reader.problem(at, format!("`paths` has no key `{other}`; it has: renders")),
                    }
                }
            }
            "render" => {
                for (key, at, value) in reader.entries("`render`", value) {
                    match key.as_str() {
                        "output" => project.output = reader.path("render.output", value),
                        "backend" => {
                            let read = reader.text("render.backend", value);
                            match read.as_deref().map(Renderer::from_name) {
                                Some(Some(renderer)) => project.renderer = renderer,
                                Some(None) => reader.problem(value, "`render.backend` must be one of: auto, cpu, gpu".to_string()),
                                None => {}
                            }
                        }
                        "quality" => {
                            let read = reader.text("render.quality", value);
                            match read.as_deref().map(Quality::from_name) {
                                Some(Some(quality)) => project.quality = quality,
                                Some(None) => {
                                    reader.problem(value, "`render.quality` must be one of: draft, standard, high".to_string());
                                }
                                None => {}
                            }
                        }
                        "encoder" => {
                            let read = reader.text("render.encoder", value);
                            match read.as_deref().map(Coder::from_name) {
                                Some(Some(coder)) => project.coder = coder,
                                Some(None) => reader.problem(value, "`render.encoder` must be one of: hardware, software".to_string()),
                                None => {}
                            }
                        }
                        "codec" => {
                            let read = reader.text("render.codec", value);
                            match read.as_deref().map(Codec::from_name) {
                                Some(Some(codec)) => project.codec = codec,
                                Some(None) => reader.problem(value, "`render.codec` must be one of: h264, hevc".to_string()),
                                None => {}
                            }
                        }
                        "workers" => {
                            let read = match &value.data {
                                YamlData::Value(Scalar::Integer(count)) => Workers::from_name(&count.to_string()),
                                YamlData::Value(Scalar::String(text)) => Workers::from_name(text),
                                _ => None,
                            };
                            match read {
                                Some(workers) => project.workers = workers,
                                None => reader.problem(
                                    value,
                                    format!("`render.workers` must be `auto` or a whole number from 1 to {}", Workers::MOST),
                                ),
                            }
                        }
                        "audio-bitrate" => match &value.data {
                            YamlData::Value(Scalar::Integer(rate)) if (8..=512).contains(rate) => project.audio_bitrate = *rate as u32,
                            _ => reader.problem(
                                value,
                                "`render.audio-bitrate` must be a whole number of kbit/s from 8 to 512".to_string(),
                            ),
                        },
                        other => {
                            let known = "output, backend, quality, encoder, codec, workers, audio-bitrate";
                            reader.problem(at, format!("`render` has no key `{other}`; it has: {known}"));
                        }
                    }
                }
            }
            other => {
                let known = "name, composition, paths, render";
                reader.problem(at, format!("the project file has no key `{other}`; it has: {known}"));
            }
        }
    }
    if reader.problems.is_empty() {
        Ok(project)
    } else {
        Err(reader.problems)
    }
}

fn read(file: &Path, folder: &Path) -> Result<Project, String> {
    let source = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    parse(&source, folder).map_err(|problems| {
        let lines: Vec<String> = problems.iter().map(|p| format!("  line {}: {}", p.line, p.message)).collect();
        format!("{} is not a valid project file:\n{}", file.display(), lines.join("\n"))
    })
}

/// The project that `target` belongs to, and the composition to render. `target` is a project
/// folder, a project file, or a composition.
pub fn open(target: &Path) -> Result<(Project, PathBuf), String> {
    if target.is_dir() {
        let file = target.join(FILE_NAME);
        let project = if file.is_file() {
            read(&file, target)?
        } else {
            Project::defaults(target)
        };
        let composition = project.folder.join(&project.composition);
        return Ok((project, composition));
    }
    if !target.is_file() {
        return Err(format!(
            "{} is not a project folder, a project file or a composition",
            target.display()
        ));
    }
    let own_folder = target
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if target.file_name().is_some_and(|name| name == FILE_NAME) {
        let project = read(target, own_folder)?;
        let composition = project.folder.join(&project.composition);
        return Ok((project, composition));
    }
    // A composition: its project is the nearest folder at or above it with a project file.
    let absolute = own_folder
        .canonicalize()
        .map_err(|error| format!("{}: {error}", own_folder.display()))?;
    for folder in absolute.ancestors() {
        let file = folder.join(FILE_NAME);
        if file.is_file() {
            return Ok((read(&file, folder)?, target.to_path_buf()));
        }
    }
    Ok((Project::defaults(own_folder), target.to_path_buf()))
}
