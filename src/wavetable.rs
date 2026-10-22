use realfft::RealFftPlanner;
use std::f32::consts::PI;

pub const SIZE: usize = 2048;
pub const FRAMES: usize = 64;
pub const LEVELS: usize = 11;
const STRIDE: usize = SIZE + 1;
const MAX_HARMONICS: usize = SIZE / 2;

pub struct WaveTable {
    data: Vec<f32>,
}

fn hash01(n: usize) -> f32 {
    let h = (n as u32).wrapping_mul(2654435761) >> 16;
    (h & 255) as f32 / 255.0
}

fn keyframe(key: usize, n: usize) -> f32 {
    let nf = n as f32;
    let odd = n % 2 == 1;
    match key {
        0 => {
            if n == 1 {
                1.0
            } else {
                0.0
            }
        }
        1 => {
            if odd {
                let sign = if ((n - 1) / 2) % 2 == 0 { 1.0 } else { -1.0 };
                sign / (nf * nf)
            } else {
                0.0
            }
        }
        2 => 1.0 / nf.powf(1.3),
        3 => {
            if odd {
                1.0 / nf
            } else {
                0.0
            }
        }
        4 => (nf * PI * 0.125).sin() / nf,
        5 => match n {
            1 => 1.0,
            2 => 0.8,
            3 => 0.6,
            4 => 0.5,
            6 => 0.4,
            8 => 0.3,
            _ => 0.0,
        },
        6 => {
            let peak1 = (-((nf - 6.0) / 2.0).powi(2)).exp();
            let peak2 = (-((nf - 14.0) / 3.0).powi(2)).exp();
            nf.powf(-0.8) * (1.0 + 4.0 * peak1 + 3.0 * peak2)
        }
        _ => {
            if n <= 24 {
                let a = hash01(n);
                a * a / nf.powf(1.1)
            } else {
                0.0
            }
        }
    }
}

const KEYS: usize = 8;

fn taper(n: usize, max_h: usize) -> f32 {
    if max_h < 16 {
        return 1.0;
    }
    let x = n as f32 / max_h as f32;
    if x < 0.75 {
        1.0
    } else {
        let t = ((x - 0.75) / 0.3125).min(1.0);
        0.5 * (1.0 + (PI * t).cos())
    }
}

impl WaveTable {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let ifft = planner.plan_fft_inverse(SIZE);
        let mut data = Vec::with_capacity(FRAMES * LEVELS * STRIDE);
        let mut spectrum = ifft.make_input_vec();
        let mut wave = ifft.make_output_vec();
        let mut amps = vec![0.0f32; MAX_HARMONICS];

        for frame in 0..FRAMES {
            let t = frame as f32 / (FRAMES - 1) as f32 * (KEYS - 1) as f32;
            let k0 = (t.floor() as usize).min(KEYS - 2);
            let frac = t - k0 as f32;
            for n in 1..MAX_HARMONICS {
                let a = keyframe(k0, n);
                let b = keyframe(k0 + 1, n);
                amps[n] = a + (b - a) * frac;
            }

            let mut gain = 1.0f32;
            for level in 0..LEVELS {
                let max_h = ((MAX_HARMONICS >> level).max(1)).min(MAX_HARMONICS - 1);
                for bin in spectrum.iter_mut() {
                    bin.re = 0.0;
                    bin.im = 0.0;
                }
                for n in 1..=max_h {
                    spectrum[n].im = -amps[n] * 0.5 * taper(n, max_h);
                }
                ifft.process(&mut spectrum, &mut wave).expect("fft");
                if level == 0 {
                    let peak = wave.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
                    gain = 0.9 / peak;
                }
                data.extend(wave.iter().map(|v| v * gain));
                data.push(wave[0] * gain);
            }
        }

        Self { data }
    }

    pub fn level_for(&self, freq_norm: f32) -> usize {
        let allowed = (0.5 / freq_norm.max(1e-6)).max(1.0);
        let k = (MAX_HARMONICS as f32 / allowed).log2().ceil().max(0.0) as usize;
        k.min(LEVELS - 1)
    }

    pub fn offsets(&self, position: f32, level: usize) -> (usize, usize, f32) {
        let fpos = position.clamp(0.0, 1.0) * (FRAMES - 1) as f32;
        let i0 = (fpos.floor() as usize).min(FRAMES - 1);
        let i1 = (i0 + 1).min(FRAMES - 1);
        (
            (i0 * LEVELS + level) * STRIDE,
            (i1 * LEVELS + level) * STRIDE,
            fpos - i0 as f32,
        )
    }

    pub fn preview(&self, position: f32, out: &mut [f32]) {
        let (off0, off1, frac) = self.offsets(position, 0);
        let n = out.len().max(1);
        for (i, o) in out.iter_mut().enumerate() {
            *o = self.read(off0, off1, frac, i as f32 / n as f32);
        }
    }

    #[inline]
    pub fn read(&self, off0: usize, off1: usize, wfrac: f32, phase: f32) -> f32 {
        let x = phase * SIZE as f32;
        let i = x as usize;
        let fr = x - i as f32;
        let a0 = self.data[off0 + i];
        let a1 = self.data[off0 + i + 1];
        let b0 = self.data[off1 + i];
        let b1 = self.data[off1 + i + 1];
        let a = a0 + (a1 - a0) * fr;
        let b = b0 + (b1 - b0) * fr;
        a + (b - a) * wfrac
    }
}

impl Default for WaveTable {
    fn default() -> Self {
        Self::new()
    }
}
