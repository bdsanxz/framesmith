//! The finishes of an element's picture (`docs/spec/finishes.md`): the fractal noise they read, a torn edge (`displace`)
//! and grain.

/// The noise `noise(FREQUENCY OCTAVES SEED)` of `docs/spec/finishes.md`, section 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    pub frequency: f64,
    pub octaves: u32,
    pub seed: i64,
}

/// The finishes of an element, in the element's own pixels, with the top left corner of its rectangle in the space of its
/// marks (`origin`), from which the noise is read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish {
    pub origin: (f64, f64),
    /// `displace="AMOUNT noise(...)"`: the amount and the noise.
    pub displace: Option<(f64, Noise)>,
    /// `grain="noise(...) SLOPE OFFSET"`: the noise, the slope and the offset.
    pub grain: Option<(Noise, f64, f64)>,
}

const M: i64 = 2_147_483_647;
const A: i64 = 16_807;
const Q: i64 = 127_773;
const R: i64 = 2_836;
const SIZE: usize = 256;

/// The lattice of a noise, drawn once from its seed: the shuffled list and the gradients of the fourth channel, each
/// repeated (section 1, step 3).
#[derive(Clone, Debug)]
pub struct Field {
    noise: Noise,
    list: [usize; SIZE + SIZE + 2],
    gradients: [[f64; 2]; SIZE + SIZE + 2],
}

/// The seed as the standard takes it (section 1, step 1).
fn first(seed: i64) -> i64 {
    if seed <= 0 {
        -(seed % (M - 1)) + 1
    } else if seed > M - 1 {
        M - 1
    } else {
        seed
    }
}

/// The next random number (section 1, step 2).
fn next(r: i64) -> i64 {
    let r = A * (r % Q) - R * (r / Q);
    if r <= 0 {
        r + M
    } else {
        r
    }
}

impl Noise {
    /// The field of this noise, ready to be read.
    pub fn field(self) -> Field {
        let mut r = first(self.seed);
        let mut gradients = [[0.0; 2]; SIZE + SIZE + 2];
        for channel in 0..4 {
            for gradient in gradients.iter_mut().take(SIZE) {
                let mut drawn = [0.0; 2];
                for value in &mut drawn {
                    r = next(r);
                    *value = ((r % (SIZE as i64 + SIZE as i64)) - SIZE as i64) as f64 / SIZE as f64;
                }
                let length = (drawn[0] * drawn[0] + drawn[1] * drawn[1]).sqrt();
                // Only the fourth channel, the alpha, is read; the others are drawn to keep the order of the numbers.
                if channel == 3 {
                    *gradient = [drawn[0] / length, drawn[1] / length];
                }
            }
        }
        let mut list = [0; SIZE + SIZE + 2];
        for (i, entry) in list.iter_mut().enumerate().take(SIZE) {
            *entry = i;
        }
        for i in (1..SIZE).rev() {
            r = next(r);
            list.swap(i, (r % SIZE as i64) as usize);
        }
        for i in 0..SIZE + 2 {
            list[SIZE + i] = list[i];
            gradients[SIZE + i] = gradients[i];
        }
        Field {
            noise: self,
            list,
            gradients,
        }
    }
}

fn s_curve(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn mix(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

impl Field {
    /// The noise of one octave at `(u, v)` (section 1, step 4).
    fn octave(&self, u: f64, v: f64) -> f64 {
        let split = |value: f64| {
            let t = value + 4096.0;
            let whole = t.trunc();
            let low = (whole as i64 & 255) as usize;
            (low, (low + 1) & 255, t - whole, t - whole - 1.0)
        };
        let (x0, x1, rx0, rx1) = split(u);
        let (y0, y1, ry0, ry1) = split(v);
        let (i, j) = (self.list[x0], self.list[x1]);
        let dot = |corner: usize, x: f64, y: f64| {
            let gradient = self.gradients[corner];
            gradient[0] * x + gradient[1] * y
        };
        let (sx, sy) = (s_curve(rx0), s_curve(ry0));
        let a = mix(sx, dot(self.list[i + y0], rx0, ry0), dot(self.list[j + y0], rx1, ry0));
        let b = mix(sx, dot(self.list[i + y1], rx0, ry1), dot(self.list[j + y1], rx1, ry1));
        mix(sy, a, b)
    }

    /// The noise at `(x, y)` of the element's box, in 256 steps (section 1, steps 5 and 6).
    pub fn value(&self, x: f64, y: f64) -> f64 {
        let (mut u, mut v) = (self.noise.frequency * x, self.noise.frequency * y);
        let (mut sum, mut ratio) = (0.0, 1.0);
        for _ in 0..self.noise.octaves {
            sum += self.octave(u, v) / ratio;
            u *= 2.0;
            v *= 2.0;
            ratio *= 2.0;
        }
        let n = ((sum * 255.0 + 255.0) / 2.0 / 255.0).clamp(0.0, 1.0);
        (255.0 * n + 0.5).floor() / 255.0
    }
}
