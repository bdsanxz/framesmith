//! How an element's finished picture is mixed with what is under it (`docs/spec/blends.md`): the sixteen blends, the
//! function of each (section 4), and the colour of one pixel (section 3).

/// A blend of `mix-blend-mode`, `Normal` by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Blend {
    /// The sixteen blends, in the order of their names in the notation.
    pub const ALL: [Blend; 16] = [
        Blend::Normal,
        Blend::Multiply,
        Blend::Screen,
        Blend::Overlay,
        Blend::Darken,
        Blend::Lighten,
        Blend::ColorDodge,
        Blend::ColorBurn,
        Blend::HardLight,
        Blend::SoftLight,
        Blend::Difference,
        Blend::Exclusion,
        Blend::Hue,
        Blend::Saturation,
        Blend::Color,
        Blend::Luminosity,
    ];

    /// The name the notation writes.
    pub fn name(self) -> &'static str {
        match self {
            Blend::Normal => "normal",
            Blend::Multiply => "multiply",
            Blend::Screen => "screen",
            Blend::Overlay => "overlay",
            Blend::Darken => "darken",
            Blend::Lighten => "lighten",
            Blend::ColorDodge => "color-dodge",
            Blend::ColorBurn => "color-burn",
            Blend::HardLight => "hard-light",
            Blend::SoftLight => "soft-light",
            Blend::Difference => "difference",
            Blend::Exclusion => "exclusion",
            Blend::Hue => "hue",
            Blend::Saturation => "saturation",
            Blend::Color => "color",
            Blend::Luminosity => "luminosity",
        }
    }

    /// The blend a name stands for, or `None` for a name that is not one of the sixteen.
    pub fn named(name: &str) -> Option<Blend> {
        Blend::ALL.into_iter().find(|blend| blend.name() == name)
    }

    /// The number the card's shader knows the blend by: its place in `ALL`.
    pub fn index(self) -> u32 {
        Blend::ALL.iter().position(|blend| *blend == self).unwrap_or(0) as u32
    }

    /// `B(Cb, Cs)` of section 4: the mixed colour of `backdrop` and `source`, each channel from 0 to 1, with no alpha.
    pub fn mix(self, backdrop: [f64; 3], source: [f64; 3]) -> [f64; 3] {
        let each = |function: fn(f64, f64) -> f64| [0, 1, 2].map(|channel| function(backdrop[channel], source[channel]));
        match self {
            Blend::Normal => source,
            Blend::Multiply => each(multiply),
            Blend::Screen => each(screen),
            Blend::Overlay => each(|b, s| hard_light(s, b)),
            Blend::Darken => each(f64::min),
            Blend::Lighten => each(f64::max),
            Blend::ColorDodge => each(color_dodge),
            Blend::ColorBurn => each(color_burn),
            Blend::HardLight => each(hard_light),
            Blend::SoftLight => each(soft_light),
            Blend::Difference => each(|b, s| (b - s).abs()),
            Blend::Exclusion => each(|b, s| b + s - 2.0 * b * s),
            Blend::Hue => with_lum(with_sat(source, sat(backdrop)), lum(backdrop)),
            Blend::Saturation => with_lum(with_sat(backdrop, sat(source)), lum(backdrop)),
            Blend::Color => with_lum(source, lum(backdrop)),
            Blend::Luminosity => with_lum(backdrop, lum(source)),
        }
    }

    /// The pixel of section 3: `source`, a premultiplied pixel of the element's faded picture, mixed with `backdrop`, the
    /// premultiplied pixel under it. A source of alpha 0 leaves the backdrop.
    pub fn pixel(self, source: [u8; 4], backdrop: [u8; 4]) -> [u8; 4] {
        if source[3] == 0 {
            return backdrop;
        }
        let cs = source.map(|value| f64::from(value) / 255.0);
        let cb = backdrop.map(|value| f64::from(value) / 255.0);
        let (alpha_s, alpha_b) = (cs[3], cb[3]);
        let unmultiplied =
            |colour: [f64; 4], alpha: f64| [0, 1, 2].map(|channel| if alpha == 0.0 { 0.0 } else { (colour[channel] / alpha).min(1.0) });
        let mixed = self.mix(unmultiplied(cb, alpha_b), unmultiplied(cs, alpha_s));
        let level = |value: f64| (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
        let mut out = [0_u8; 4];
        for channel in 0..3 {
            out[channel] = level(cs[channel] * (1.0 - alpha_b) + cb[channel] * (1.0 - alpha_s) + alpha_s * alpha_b * mixed[channel]);
        }
        out[3] = level(alpha_s + alpha_b - alpha_s * alpha_b);
        out
    }
}

fn multiply(b: f64, s: f64) -> f64 {
    b * s
}

fn screen(b: f64, s: f64) -> f64 {
    b + s - b * s
}

fn hard_light(b: f64, s: f64) -> f64 {
    if s <= 0.5 {
        multiply(b, 2.0 * s)
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

fn color_dodge(b: f64, s: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else if s == 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

fn color_burn(b: f64, s: f64) -> f64 {
    if b == 1.0 {
        1.0
    } else if s == 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

fn soft_light(b: f64, s: f64) -> f64 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

fn lum(colour: [f64; 3]) -> f64 {
    0.3 * colour[0] + 0.59 * colour[1] + 0.11 * colour[2]
}

fn sat(colour: [f64; 3]) -> f64 {
    colour.iter().copied().fold(f64::MIN, f64::max) - colour.iter().copied().fold(f64::MAX, f64::min)
}

fn clip(colour: [f64; 3]) -> [f64; 3] {
    let l = lum(colour);
    let least = colour.iter().copied().fold(f64::MAX, f64::min);
    let most = colour.iter().copied().fold(f64::MIN, f64::max);
    let mut colour = colour;
    if least < 0.0 {
        colour = colour.map(|c| l + (c - l) * l / (l - least));
    }
    if most > 1.0 {
        colour = colour.map(|c| l + (c - l) * (1.0 - l) / (most - l));
    }
    colour
}

fn with_lum(colour: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(colour);
    clip(colour.map(|c| c + d))
}

fn with_sat(colour: [f64; 3], t: f64) -> [f64; 3] {
    // The channels in a stable order of their values: the least, the middle and the greatest.
    let mut order = [0, 1, 2];
    order.sort_by(|a, b| colour[*a].total_cmp(&colour[*b]));
    let [least, middle, most] = order;
    let mut out = [0.0; 3];
    if colour[most] > colour[least] {
        out[middle] = (colour[middle] - colour[least]) * t / (colour[most] - colour[least]);
        out[most] = t;
    }
    out
}
