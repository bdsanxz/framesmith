//! A table of colours in three dimensions, read from a `.cube` file. See
//! `docs/spec/native-notation.md`, section 9.

/// The fewest and the most points a table may have along one colour.
const SIZES: std::ops::RangeInclusive<usize> = 2..=128;

/// For every colour, the colour it becomes.
pub struct Lut {
    size: usize,
    /// `size * size * size` colours: red changes quickest, then green, then blue.
    points: Vec<[f32; 3]>,
    /// For each level of a channel: the point below it, and how far it is towards the next.
    places: Box<[(usize, f32); 256]>,
}

impl PartialEq for Lut {
    fn eq(&self, other: &Lut) -> bool {
        self.size == other.size && self.points == other.points
    }
}

impl std::fmt::Debug for Lut {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Lut of {} points a side", self.size)
    }
}

impl Lut {
    /// A table from its points. `points` holds `size * size * size` of them.
    fn new(size: usize, points: Vec<[f32; 3]>) -> Lut {
        let last = (size - 1) as f32;
        let places = Box::new(std::array::from_fn(|level| {
            let place = level as f32 / 255.0 * last;
            let below = (place.floor() as usize).min(size - 2);
            (below, place - below as f32)
        }));
        Lut { size, points, places }
    }

    /// The table a `.cube` text holds, or what is wrong with the text.
    pub fn read(text: &str) -> Result<Lut, String> {
        let mut size = None;
        let mut points = Vec::new();
        for (number, line) in text.lines().enumerate().map(|(index, line)| (index + 1, line.trim())) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or("");
            if first.starts_with(|letter: char| letter.is_ascii_alphabetic()) {
                let rest: Vec<&str> = words.collect();
                let range = |wanted: &str| -> Result<(), String> {
                    if rest
                        .iter()
                        .all(|word| word.parse::<f32>().is_ok_and(|value| value.to_string() == wanted))
                    {
                        Ok(())
                    } else {
                        Err(format!("line {number}: `{line}`: only tables for colours from 0 to 1 are read"))
                    }
                };
                match first {
                    "TITLE" => {}
                    "LUT_3D_SIZE" => {
                        let read = rest.first().and_then(|word| word.parse::<usize>().ok());
                        size = Some(read.filter(|size| SIZES.contains(size)).ok_or_else(|| {
                            format!(
                                "line {number}: `{line}`: the size must be a whole number from {} to {}",
                                SIZES.start(),
                                SIZES.end()
                            )
                        })?);
                    }
                    "LUT_1D_SIZE" => {
                        return Err(format!(
                            "line {number}: this is a table of one dimension; only tables of three are read"
                        ))
                    }
                    "DOMAIN_MIN" => range("0")?,
                    "DOMAIN_MAX" => range("1")?,
                    _ => return Err(format!("line {number}: `{first}` is not known")),
                }
                continue;
            }
            let read: Vec<f32> = line.split_whitespace().map_while(|word| word.parse().ok()).collect();
            match read[..] {
                [red, green, blue] if line.split_whitespace().count() == 3 && read.iter().all(|value| value.is_finite()) => {
                    points.push([red, green, blue]);
                }
                _ => return Err(format!("line {number}: `{line}` is not three numbers")),
            }
        }
        let size = size.ok_or("it has no `LUT_3D_SIZE`")?;
        if points.len() != size * size * size {
            return Err(format!(
                "it has {} rows of colours; a table of size {size} has {}",
                points.len(),
                size * size * size
            ));
        }
        Ok(Lut::new(size, points))
    }

    /// How many points the table has along one colour.
    pub fn size(&self) -> usize {
        self.size
    }

    /// The points as the card reads them: four numbers each, the last of them 1.
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.points.len() * 16);
        for point in &self.points {
            for value in point.iter().chain([&1.0]) {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes
    }

    /// The colour a colour becomes: between the eight points around it, along red first, then
    /// green, then blue.
    pub fn at(&self, [red, green, blue]: [u8; 3]) -> [u8; 3] {
        let ((r, across), (g, down), (b, deep)) = (
            self.places[usize::from(red)],
            self.places[usize::from(green)],
            self.places[usize::from(blue)],
        );
        let row = self.size;
        let from = (b * row + g) * row + r;
        // The two points along red at one green and one blue, mixed.
        let along = |from: usize| -> [f32; 3] {
            let (near, far) = (self.points[from], self.points[from + 1]);
            std::array::from_fn(|channel| near[channel] * (1.0 - across) + far[channel] * across)
        };
        let mix = |near: [f32; 3], far: [f32; 3], share: f32| -> [f32; 3] {
            std::array::from_fn(|channel| near[channel] * (1.0 - share) + far[channel] * share)
        };
        let front = mix(along(from), along(from + row), down);
        let behind = mix(along(from + row * row), along(from + row * row + row), down);
        mix(front, behind, deep).map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// Replaces the colours of RGBA pixels in place. Alpha is left as it is.
    pub fn apply(&self, pixels: &mut [u8]) {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let [red, green, blue] = self.at([pixel[0], pixel[1], pixel[2]]);
            (pixel[0], pixel[1], pixel[2]) = (red, green, blue);
        }
    }
}
