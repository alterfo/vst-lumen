use std::f32::consts::PI;

#[inline]
pub fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[inline]
pub fn morph_weights(m: f32) -> [f32; 3] {
    let m = m.clamp(0.0, 1.0);
    if m < 0.5 {
        [1.0 - 2.0 * m, 2.0 * m, 0.0]
    } else {
        [0.0, 2.0 - 2.0 * m, 2.0 * m - 1.0]
    }
}

#[derive(Clone, Copy, Default)]
pub struct Svf {
    s1: f32,
    s2: f32,
}

pub struct Coefs {
    g: f32,
    r2: f32,
    denom: f32,
    drive: f32,
    comp: f32,
    weights: [f32; 3],
}

impl Coefs {
    pub fn new(cutoff: f32, resonance: f32, drive: f32, morph: f32, sr: f32) -> Self {
        let g = (PI * cutoff / sr).tan();
        let r = 1.0 - 0.97 * resonance;
        let r2 = 2.0 * r;
        let d = 1.0 + drive * 6.0;
        Self {
            g,
            r2,
            denom: 1.0 + r2 * g + g * g,
            drive: d,
            comp: 1.0 / soft_clip(d * 0.5).max(0.1) * 0.5,
            weights: morph_weights(morph),
        }
    }
}

impl Svf {
    pub const fn new() -> Self {
        Self { s1: 0.0, s2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, input: f32, c: &Coefs) -> f32 {
        let x = soft_clip(input * c.drive) * c.comp;
        let hp = (x - (c.r2 + c.g) * self.s1 - self.s2) / c.denom;
        let bp = c.g * hp + self.s1;
        self.s1 = c.g * hp + bp;
        let lp = c.g * bp + self.s2;
        self.s2 = c.g * bp + lp;
        lp * c.weights[0] + bp * c.r2 * c.weights[1] + hp * c.weights[2]
    }
}
