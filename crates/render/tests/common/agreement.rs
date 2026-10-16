//! The comparison of the card's pictures with the processor's, as `docs/spec/card-renderer.md`
//! sections 4 and 7 state it. The tests use it, and the development example `compare_renderers`
//! includes this file by its path, so it depends on nothing that only a test has.

use draw::{Picture, Renderer};
use measure::Frame;
use render::Scene;

/// The least similarity of the two pictures of a frame (`PLAN.md` section 5.2).
pub const LEAST_SIMILARITY: f64 = 0.995;

/// A colour value further apart than this many levels of 255 is counted. More than this, not this
/// or more: a value 4 levels apart is not counted.
pub const MOST_LEVELS_APART: u8 = 4;

/// The share of counted values allowed, as its divisor: 0.05 % is one value in 2000, so a count `n`
/// of `t` values is within the tolerance when `n * ONE_IN <= t`.
pub const ONE_IN: u64 = 2000;

/// The levels the doctored card raises the red value of every pixel by (`card-renderer.md` section 7).
pub const DOCTORED_LEVELS: u8 = 40;

/// The figures of one frame: the similarity of the two pictures, and how many of their colour
/// values are further apart than `MOST_LEVELS_APART`, out of all of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Figures {
    pub number: u64,
    /// `None` when the similarity cannot be measured (`measure::similarity`).
    pub similarity: Option<f64>,
    pub further_apart: u64,
    /// The red, green and blue values of every pixel: width times height times 3.
    pub values: u64,
}

impl Figures {
    /// Whether the similarity is measured and is `LEAST_SIMILARITY` or better.
    pub fn similarity_within(&self) -> bool {
        matches!(self.similarity, Some(similarity) if similarity >= LEAST_SIMILARITY)
    }

    /// Whether the counted values are within the share allowed, in whole numbers.
    pub fn share_within(&self) -> bool {
        self.further_apart * ONE_IN <= self.values
    }

    /// Whether the frame is within the tolerance on both figures.
    pub fn within(&self) -> bool {
        self.similarity_within() && self.share_within()
    }

    /// The share of counted values, in per cent.
    pub fn share_percent(&self) -> f64 {
        100.0 * self.further_apart as f64 / self.values as f64
    }

    /// The line of the report of this frame (`card-renderer.md` section 4.1).
    pub fn line(&self) -> String {
        let similarity = match self.similarity {
            Some(similarity) => format!("{similarity:.6}"),
            None => "not measured".to_string(),
        };
        let verdict = if self.within() {
            "within the tolerance"
        } else {
            "outside the tolerance"
        };
        format!(
            "frame {:04}: similarity {similarity}, values further apart than 4 levels {:.4} % ({} of {}), {verdict}",
            self.number,
            self.share_percent(),
            self.further_apart,
            self.values
        )
    }

    /// The words of each figure that misses the tolerance, with the figure in them.
    pub fn misses(&self) -> Vec<String> {
        let mut misses = Vec::new();
        match self.similarity {
            Some(similarity) if similarity < LEAST_SIMILARITY => {
                misses.push(format!("similarity {similarity:.6} is below {LEAST_SIMILARITY}"));
            }
            Some(_) => {}
            None => misses.push("similarity could not be measured".to_string()),
        }
        if !self.share_within() {
            misses.push(format!(
                "{:.4} % of values are further apart than 4 levels, above 0.05 %",
                self.share_percent()
            ));
        }
        misses
    }
}

/// The figures of frame `number`: the card's picture against the processor's. Two pictures of
/// different sizes cannot be compared, and that is an error and not a figure.
pub fn figures(number: u64, processor: &Picture, card: &Picture) -> Result<Figures, String> {
    if (card.width(), card.height()) != (processor.width(), processor.height()) {
        return Err(format!(
            "frame {number:04}: the card's picture is {}x{} and the processor's is {}x{}",
            card.width(),
            card.height(),
            processor.width(),
            processor.height()
        ));
    }
    let frame = |picture: &Picture| Frame {
        width: picture.width(),
        height: picture.height(),
        rgb: picture
            .pixels()
            .chunks(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect(),
    };
    let similarity = measure::similarity(&frame(processor), &frame(card));
    let further_apart = processor
        .pixels()
        .chunks(4)
        .zip(card.pixels().chunks(4))
        .map(|(a, b)| {
            a[..3]
                .iter()
                .zip(&b[..3])
                .filter(|&(x, y)| x.abs_diff(*y) > MOST_LEVELS_APART)
                .count() as u64
        })
        .sum();
    let values = u64::from(processor.width()) * u64::from(processor.height()) * 3;
    Ok(Figures {
        number,
        similarity,
        further_apart,
        values,
    })
}

/// The picture with the red value of every pixel raised by `levels`, stopped at 255. It makes the
/// card that must fail the comparison (`card-renderer.md` section 7): a device of the comparison,
/// not a setting of the product.
pub fn raise_red(picture: &Picture, levels: u8) -> Picture {
    let pixels: Vec<u8> = picture
        .pixels()
        .chunks(4)
        .flat_map(|pixel| [pixel[0].saturating_add(levels), pixel[1], pixel[2], pixel[3]])
        .collect();
    Picture::new(picture.width(), picture.height(), pixels).expect("the picture keeps its size")
}

/// Draws frame `number` of the scene on both renderers and compares the two pictures. When
/// `doctored` is that frame, the card's picture of it is raised with `raise_red` first.
pub fn compare_frame(
    scene: &Scene,
    number: u64,
    processor: &mut dyn Renderer,
    card: &mut dyn Renderer,
    doctored: Option<u64>,
) -> Result<Figures, String> {
    let expected = scene
        .picture(number, &mut *processor)
        .map_err(|error| format!("frame {number:04}: the processor could not draw it: {error}"))?;
    let mut drawn = scene
        .picture(number, &mut *card)
        .map_err(|error| format!("frame {number:04}: the card could not draw it: {error}"))?;
    if doctored == Some(number) {
        drawn = raise_red(&drawn, DOCTORED_LEVELS);
    }
    figures(number, &expected, &drawn)
}

/// The comparison of every frame of a scene, in order.
pub fn compare_scene(scene: &Scene, processor: &mut dyn Renderer, card: &mut dyn Renderer, doctored: Option<u64>) -> Comparison {
    let frames = (0..scene.frames())
        .map(|number| compare_frame(scene, number, &mut *processor, &mut *card, doctored))
        .collect();
    Comparison { frames }
}

/// The figures of a comparison, frame by frame: one entry for each frame, which is an error when the
/// frame could not be compared at all.
pub struct Comparison {
    pub frames: Vec<Result<Figures, String>>,
}

impl Comparison {
    /// Whether every frame is within the tolerance. A frame that could not be compared is not.
    pub fn passed(&self) -> bool {
        self.frames.iter().all(|frame| matches!(frame, Ok(figures) if figures.within()))
    }

    /// The report: a line for each frame in order, then the frames that miss the tolerance with
    /// the figures they miss, then the line that says whether the comparison passed.
    pub fn report(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .frames
            .iter()
            .map(|frame| match frame {
                Ok(figures) => figures.line(),
                Err(error) => format!("{error}; it is outside the tolerance"),
            })
            .collect();
        for figures in self.frames.iter().flatten() {
            if !figures.within() {
                lines.push(format!(
                    "frame {:04} is outside the tolerance: {}",
                    figures.number,
                    figures.misses().join("; ")
                ));
            }
        }
        let outside = self
            .frames
            .iter()
            .filter(|frame| !matches!(frame, Ok(figures) if figures.within()))
            .count();
        lines.push(if outside == 0 {
            format!("the comparison passed: all {} frames are within the tolerance", self.frames.len())
        } else {
            format!(
                "the comparison failed: {outside} of {} frames are outside the tolerance",
                self.frames.len()
            )
        });
        lines
    }
}
