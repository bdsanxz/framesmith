//! The `<image>` of a composition (`docs/spec/still-pictures.md`): what it takes, its file read through frx7, and the box
//! its picture is drawn in.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use base::{Error, PixelFormat};
use draw::{Fit, Still};
use layout::{Amount, Beyond, Rect, Written};
use project::{Code, Node, Project};
use timebase::Time;

/// What an `<image>` takes in this version (`docs/spec/still-pictures.md`, section 1).
pub(crate) const IMAGE_TAKES: [&str; 14] = [
    "id",
    "class",
    "start",
    "duration",
    "left",
    "top",
    "right",
    "bottom",
    "width",
    "height",
    "position",
    "src",
    "object-fit",
    "object-position",
];

/// Why a picture file is not drawn: the code and the message, without the file's name, which the element's report gives.
#[derive(Clone, Debug)]
pub(crate) struct Refused {
    pub code: Code,
    pub reason: String,
}

/// The picture files of a run, each read once, by the path it resolves to.
#[derive(Default)]
pub(crate) struct Stills {
    codecs: Option<codec::Registry>,
    read: HashMap<PathBuf, Result<Arc<Still>, Refused>>,
}

impl Stills {
    /// The picture of the file `src` names, read through frx7 (section 2), or why it is not drawn (section 3).
    pub fn picture(&mut self, project: &Project, src: &str) -> Result<Arc<Still>, Refused> {
        let path = project.resolve(src);
        if let Some(found) = self.read.get(&path) {
            return found.clone();
        }
        let codecs = self.codecs.get_or_insert_with(|| {
            let mut codecs = codec::Registry::new();
            codecs.register(backend_os::BACKEND);
            codecs
        });
        let found = read(codecs, &path, src);
        self.read.insert(path, found.clone());
        found
    }
}

/// Reads one picture file: its facts first, then its pixels, premultiplied.
fn read(codecs: &codec::Registry, path: &std::path::Path, src: &str) -> Result<Arc<Still>, Refused> {
    let shown = project::shown(src);
    if !path.is_file() {
        return Err(Refused {
            code: Code::PictureMissing,
            reason: format!("`src=\"{shown}\"` names no file: a picture file, from the project folder or a path that begins with `/`"),
        });
    }
    let facts = codecs.still_facts(path).map_err(|error| refused(&shown, &error))?;
    if facts.turned() {
        return Err(Refused {
            code: Code::FormNotSupported,
            reason: format!(
                "`src=\"{shown}\"` stores a turn (orientation {}), which this version does not apply (S26)",
                facts.orientation.exif()
            ),
        });
    }
    let picture = codecs.still(path).map_err(|error| refused(&shown, &error))?;
    if picture.format != PixelFormat::Rgba8 || picture.planes.is_empty() {
        return Err(Refused {
            code: Code::PictureDamaged,
            reason: format!("`src=\"{shown}\"` is damaged or cannot be read: its pixels are not given as RGBA"),
        });
    }
    let plane = &picture.planes[0];
    let row = picture.width as usize * 4;
    let mut straight = Vec::with_capacity(row * picture.height as usize);
    for line in 0..picture.height as usize {
        let start = line * plane.stride;
        match plane.data.get(start..start + row) {
            Some(found) => straight.extend_from_slice(found),
            None => {
                return Err(Refused {
                    code: Code::PictureDamaged,
                    reason: format!("`src=\"{shown}\"` is damaged or cannot be read: its pixels are fewer than its size"),
                })
            }
        }
    }
    Still::from_straight(picture.width, picture.height, &straight)
        .map(Arc::new)
        .ok_or_else(|| Refused {
            code: Code::PictureDamaged,
            reason: format!("`src=\"{shown}\"` is damaged or cannot be read: its pixels are not of its size"),
        })
}

/// The report of a refusal of frx7 (section 3): a kind not read is unsupported, anything else is damage or a read that
/// failed.
fn refused(shown: &str, error: &Error) -> Refused {
    match error {
        Error::Unsupported(reason) => Refused {
            code: Code::FormNotSupported,
            reason: format!("`src=\"{shown}\"` is not a picture of a kind this version reads ({reason}); the kinds read are PNG and JPEG"),
        },
        _ => Refused {
            code: Code::PictureDamaged,
            reason: format!("`src=\"{shown}\"` is damaged or cannot be read: {error}"),
        },
    }
}

/// What an `<image>` writes: its place and size, its file, its fit and place in the box, and its window.
pub(crate) struct ImageForm {
    pub written: Written,
    pub src: String,
    pub fit: Fit,
    pub position: (f64, f64),
    pub clip: Option<(Time, Time)>,
}

/// The fit an `object-fit` names, or `None`.
pub(crate) fn fit(value: &str) -> Option<Fit> {
    match value {
        "fill" => Some(Fit::Fill),
        "cover" => Some(Fit::Cover),
        "contain" => Some(Fit::Contain),
        _ => None,
    }
}

/// The two percentages of an `object-position`, as shares, or `None` when it is not two percentages.
pub(crate) fn position(value: &str) -> Option<(f64, f64)> {
    let mut parts = value.split_whitespace().map(|part| {
        part.strip_suffix('%')
            .and_then(|number| number.parse::<f64>().ok())
            .filter(|number| number.is_finite())
    });
    let (Some(Some(across)), Some(Some(down)), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    Some((across / 100.0, down / 100.0))
}

/// The box of an `<image>` whose picture is `width` by `height`, in its parent's rectangle (section 4): a size not known
/// is the picture's, or follows the picture's proportions from the size that is.
pub(crate) fn image_rect(parent: Rect, mut written: Written, (width, height): (u32, u32)) -> Result<Rect, Beyond> {
    let across = written.width.is_some() || (written.left.is_some() && written.right.is_some());
    let down = written.height.is_some() || (written.top.is_some() && written.bottom.is_some());
    let (width, height) = (f64::from(width), f64::from(height));
    match (across, down) {
        (false, false) => {
            written.width = Some(Amount::Pixels(width));
            written.height = Some(Amount::Pixels(height));
        }
        (true, false) => {
            written.height = Some(Amount::Pixels(0.0));
            let first = layout::place(parent, written)?;
            written.height = Some(Amount::Pixels(f64::from(first.width) * height / width));
        }
        (false, true) => {
            written.width = Some(Amount::Pixels(0.0));
            let first = layout::place(parent, written)?;
            written.width = Some(Amount::Pixels(f64::from(first.height) * width / height));
        }
        (true, true) => {}
    }
    layout::place(parent, written)
}

/// Whether the element is an `<image>` in a box that lays its children out, which this version does not place
/// (section 1).
pub(crate) fn in_a_flow(parent: &Node) -> bool {
    parent.attribute("flex-direction").is_some() || parent.attribute("columns").is_some()
}
