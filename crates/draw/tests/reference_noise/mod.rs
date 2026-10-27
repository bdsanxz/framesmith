//! A second working of the fractal noise of `docs/spec/finishes.md`, section 1, written in the tests from the W3C's Filter
//! Effects procedure step by step, with no code of the product: all four channels are drawn and kept, as the standard keeps
//! them, and a value is worked for the fourth. The tests hold the product's noise, and the pictures drawn with it, to it.
//! Its loops index as the standard's procedure does, so that the two can be read side by side.
#![allow(clippy::needless_range_loop)]

const RAND_M: i64 = 2147483647;
const RAND_A: i64 = 16807;
const RAND_Q: i64 = 127773;
const RAND_R: i64 = 2836;
const B_SIZE: i64 = 0x100;
const B_MASK: i64 = 0xff;
const PERLIN_N: f64 = 4096.0;

pub struct Reference {
    lattice: Vec<i64>,
    gradient: Vec<Vec<[f64; 2]>>,
    frequency: f64,
    octaves: u32,
}

fn setup_seed(seed: i64) -> i64 {
    let mut seed = seed;
    if seed <= 0 {
        seed = -(seed % (RAND_M - 1)) + 1;
    }
    if seed > RAND_M - 1 {
        seed = RAND_M - 1;
    }
    seed
}

fn random(seed: i64) -> i64 {
    let mut result = RAND_A * (seed % RAND_Q) - RAND_R * (seed / RAND_Q);
    if result <= 0 {
        result += RAND_M;
    }
    result
}

impl Reference {
    pub fn new(frequency: f64, octaves: u32, seed: i64) -> Reference {
        let size = B_SIZE as usize;
        let mut lattice = vec![0_i64; size + size + 2];
        let mut gradient = vec![vec![[0.0_f64; 2]; size + size + 2]; 4];
        let mut seed = setup_seed(seed);
        for k in 0..4 {
            for i in 0..size {
                lattice[i] = i as i64;
                for j in 0..2 {
                    seed = random(seed);
                    gradient[k][i][j] = ((seed % (B_SIZE + B_SIZE)) - B_SIZE) as f64 / B_SIZE as f64;
                }
                let s = (gradient[k][i][0] * gradient[k][i][0] + gradient[k][i][1] * gradient[k][i][1]).sqrt();
                gradient[k][i][0] /= s;
                gradient[k][i][1] /= s;
            }
        }
        let mut i = size;
        loop {
            i -= 1;
            if i == 0 {
                break;
            }
            let k = lattice[i];
            seed = random(seed);
            let j = (seed % B_SIZE) as usize;
            lattice[i] = lattice[j];
            lattice[j] = k;
        }
        for i in 0..size + 2 {
            lattice[size + i] = lattice[i];
            for k in 0..4 {
                gradient[k][size + i] = gradient[k][i];
            }
        }
        Reference {
            lattice,
            gradient,
            frequency,
            octaves,
        }
    }

    fn noise2(&self, channel: usize, vec: [f64; 2]) -> f64 {
        let t = vec[0] + PERLIN_N;
        let bx0 = (t as i64) & B_MASK;
        let bx1 = (bx0 + 1) & B_MASK;
        let rx0 = t - (t as i64) as f64;
        let rx1 = rx0 - 1.0;
        let t = vec[1] + PERLIN_N;
        let by0 = (t as i64) & B_MASK;
        let by1 = (by0 + 1) & B_MASK;
        let ry0 = t - (t as i64) as f64;
        let ry1 = ry0 - 1.0;
        let i = self.lattice[bx0 as usize];
        let j = self.lattice[bx1 as usize];
        let b00 = self.lattice[(i + by0) as usize] as usize;
        let b10 = self.lattice[(j + by0) as usize] as usize;
        let b01 = self.lattice[(i + by1) as usize] as usize;
        let b11 = self.lattice[(j + by1) as usize] as usize;
        let sx = rx0 * rx0 * (3.0 - 2.0 * rx0);
        let sy = ry0 * ry0 * (3.0 - 2.0 * ry0);
        let lerp = |t: f64, a: f64, b: f64| a + t * (b - a);
        let g = &self.gradient[channel];
        let u = rx0 * g[b00][0] + ry0 * g[b00][1];
        let v = rx1 * g[b10][0] + ry0 * g[b10][1];
        let a = lerp(sx, u, v);
        let u = rx0 * g[b01][0] + ry1 * g[b01][1];
        let v = rx1 * g[b11][0] + ry1 * g[b11][1];
        let b = lerp(sx, u, v);
        lerp(sy, a, b)
    }

    /// The fourth channel of fractal noise at `point`, as a level from 0 to 255.
    pub fn level(&self, point: [f64; 2]) -> u8 {
        let mut vec = [point[0] * self.frequency, point[1] * self.frequency];
        let mut sum = 0.0;
        let mut ratio = 1.0;
        for _ in 0..self.octaves {
            sum += self.noise2(3, vec) / ratio;
            vec[0] *= 2.0;
            vec[1] *= 2.0;
            ratio *= 2.0;
        }
        let alpha = ((sum * 255.0 + 255.0) / 2.0).clamp(0.0, 255.0);
        (alpha + 0.5).floor() as u8
    }

    /// The same as a number from 0 to 1.
    pub fn value(&self, point: [f64; 2]) -> f64 {
        f64::from(self.level(point)) / 255.0
    }
}
