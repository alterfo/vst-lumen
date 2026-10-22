//! Organic drift source for "imperfect" analog/acoustic behaviour.
//!
//! Real instruments never hold a parameter perfectly still: string tension
//! creeps with temperature, a player's embouchure wanders, tape speed breathes.
//! Those fluctuations are not white noise (far too jittery) and not a periodic
//! LFO (far too predictable). They are *correlated* random walks whose power
//! falls off with frequency. This module layers three such processes:
//!
//! 1. Brownian drift (Ornstein-Uhlenbeck): power ~ 1/f^2. Slow, meandering,
//!    "something heavy is slowly changing" (thermal detune, bow pressure).
//! 2. Pink noise (Voss-McCartney): power ~ 1/f. Equal energy per octave, the
//!    statistics of most natural fluctuation (heart rate, flicker noise,
//!    musician timing error). Perceived as "alive" rather than "random".
//! 3. Gradient (Perlin-style) noise: smooth, band-limited, never repeats on a
//!    musical timescale, and has no tuned peak the ear can lock onto.
//!
//! Everything lives in fixed-size arrays and plain integers. No heap, no
//! locks, no `std` calls that could allocate: safe to call on the audio thread.

pub const CHANNELS: usize = 4;
const PINK_ROWS: usize = 8;

#[derive(Clone, Copy)]
pub struct OrganicParams {
    pub rate_hz: f32,
    pub brown: f32,
    pub pink: f32,
    pub perlin: f32,
}

impl Default for OrganicParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.15,
            brown: 0.5,
            pink: 0.35,
            perlin: 0.15,
        }
    }
}

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [-1, 1).
    #[inline]
    fn uniform(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 8388608.0 - 1.0
    }

    /// Approximately N(0, 1).
    ///
    /// Irwin-Hall: the sum of 4 uniform [-1, 1) variables has variance
    /// 4 * (1/3) = 4/3, so scaling by sqrt(3/4) gives unit variance. The
    /// result is bell-shaped enough for drift, and far cheaper than
    /// Box-Muller (no ln/sqrt/cos in the hot path).
    #[inline]
    fn gaussian(&mut self) -> f32 {
        (self.uniform() + self.uniform() + self.uniform() + self.uniform()) * 0.866_025_4
    }
}

/// Ornstein-Uhlenbeck process: dx = -theta * x * dt + sigma * dW.
///
/// A plain random walk (integrated white noise) wanders off without bound, so
/// a pitch driven by it would eventually be a tritone flat. OU adds a pull
/// toward zero proportional to the distance, which keeps the drift bounded
/// while preserving the smooth, meandering character of Brownian motion.
///
/// Solved *exactly* per step instead of with Euler:
///   x[n+1] = a * x[n] + sqrt(1 - a^2) * N(0, 1),   a = exp(-theta * dt)
/// This keeps the stationary variance at exactly 1 for any dt, so the output
/// level does not depend on the block size or the sample rate, and it is
/// unconditionally stable (|a| < 1) however large dt becomes.
#[derive(Clone, Copy)]
struct Brownian {
    x: f32,
}

impl Brownian {
    #[inline]
    fn step(&mut self, a: f32, noise: f32) -> f32 {
        self.x = a * self.x + (1.0 - a * a).sqrt() * noise;
        self.x
    }
}

/// Voss-McCartney pink noise generator.
///
/// PINK_ROWS independent white-noise rows are summed. Row k is refreshed only
/// every 2^k ticks, so row k contributes noise whose spectrum is flat up to
/// roughly rate / 2^k and silent above it. Stacking octave-spaced bands with
/// equal power gives a spectrum that rolls off at 3 dB per octave: 1/f.
/// Which row to refresh is picked by counting trailing zeros of a tick
/// counter, so each tick touches exactly one row: O(1), no branching.
#[derive(Clone, Copy)]
struct Pink {
    rows: [f32; PINK_ROWS],
    counter: u32,
}

impl Pink {
    #[inline]
    fn tick(&mut self, rng: &mut Rng) -> f32 {
        self.counter = self.counter.wrapping_add(1);
        let row = (self.counter.trailing_zeros() as usize).min(PINK_ROWS - 1);
        self.rows[row] = rng.uniform();
        let sum: f32 = self.rows.iter().sum();
        sum * (1.0 / (PINK_ROWS as f32 * 0.57735))
    }
}

/// Integer hash (Wang) turned into a gradient in [-1, 1].
///
/// Gradient noise needs a random slope at every integer lattice point. Hashing
/// the lattice index means we store nothing: the "table" is a pure function,
/// the noise never repeats within any session, and it costs no memory.
#[inline]
fn gradient(index: i32, seed: u32) -> f32 {
    let mut h = (index as u32) ^ seed;
    h = (h ^ 61) ^ (h >> 16);
    h = h.wrapping_mul(9);
    h ^= h >> 4;
    h = h.wrapping_mul(0x27d4_eb2d);
    h ^= h >> 15;
    (h >> 8) as f32 / 8388608.0 - 1.0
}

/// 1D Perlin noise with the quintic fade 6t^5 - 15t^4 + 10t^3.
///
/// The quintic has zero first AND second derivative at the lattice points, so
/// the result is C2-continuous: no audible corners in a slowly swept filter or
/// pitch, which a linear or cubic-hermite interpolation would leak as
/// faint zipper-like artefacts.
#[inline]
fn perlin(t: f32, seed: u32) -> f32 {
    let i = t.floor();
    let f = t - i;
    let i = i as i32;
    let fade = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let g0 = gradient(i, seed) * f;
    let g1 = gradient(i + 1, seed) * (f - 1.0);
    (g0 + (g1 - g0) * fade) * 2.0
}

/// Three-octave fractal sum: each octave doubles in rate and halves in
/// amplitude. Slow wander dominates, small faster ripples add the sense of
/// "hand tremor" without ever producing a tuned vibrato.
#[inline]
fn perlin_fbm(t: f32, seed: u32) -> f32 {
    (perlin(t, seed) + 0.5 * perlin(t * 2.07, seed ^ 0x9E37_79B9) + 0.25 * perlin(t * 4.13, seed ^ 0x85EB_CA6B))
        * (1.0 / 1.75)
}

#[derive(Clone, Copy)]
struct Channel {
    rng: Rng,
    seed: u32,
    brown: Brownian,
    pink: Pink,
    pink_smooth: f32,
    pink_phase: f32,
    perlin_t: f32,
}

impl Channel {
    fn new(seed: u32) -> Self {
        let mut c = Self {
            rng: Rng(seed | 1),
            seed,
            brown: Brownian { x: 0.0 },
            pink: Pink {
                rows: [0.0; PINK_ROWS],
                counter: 0,
            },
            pink_smooth: 0.0,
            pink_phase: 0.0,
            perlin_t: 0.0,
        };
        c.reset();
        c
    }

    fn reset(&mut self) {
        self.rng = Rng(self.seed | 1);
        self.brown.x = self.rng.gaussian();
        self.pink = Pink {
            rows: [0.0; PINK_ROWS],
            counter: 0,
        };
        for r in self.pink.rows.iter_mut() {
            *r = self.rng.uniform();
        }
        self.pink_smooth = 0.0;
        self.pink_phase = 0.0;
        self.perlin_t = (self.seed >> 8) as f32 * 0.001;
    }
}

/// Bank of CHANNELS decorrelated organic drift signals in [-1, 1].
///
/// Intended to be advanced once per control block (e.g. every 32 samples) and
/// the result applied as a block-constant modulation, with any further
/// smoothing done by the destination parameter.
pub struct OrganicModulator {
    sample_rate: f32,
    channels: [Channel; CHANNELS],
}

impl OrganicModulator {
    pub fn new(seed: u32) -> Self {
        let mut channels = [Channel::new(1); CHANNELS];
        for (i, c) in channels.iter_mut().enumerate() {
            *c = Channel::new(seed.wrapping_mul(0x9E37_79B1).wrapping_add((i as u32).wrapping_mul(0x85EB_CA6B).wrapping_add(1)));
        }
        Self {
            sample_rate: 44100.0,
            channels,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        for c in self.channels.iter_mut() {
            c.reset();
        }
    }

    /// Advances every channel by `frames` samples and returns the new values.
    ///
    /// Each channel runs at a slightly different speed (golden-ratio-spaced
    /// multipliers) so the channels never phase-lock; with identical rates two
    /// "independent" drifts would eventually look like one slowly beating
    /// pattern, which sounds like an LFO instead of a living instrument.
    pub fn process_block(&mut self, frames: usize, p: &OrganicParams) -> [f32; CHANNELS] {
        let dt = frames as f32 / self.sample_rate;
        let norm = 1.0 / (p.brown.abs() + p.pink.abs() + p.perlin.abs()).max(1e-3);
        let mut out = [0.0f32; CHANNELS];

        for (i, c) in self.channels.iter_mut().enumerate() {
            let speed = p.rate_hz * (1.0 + 0.618 * i as f32);

            let a = (-speed * dt).exp();
            let brown = c.brown.step(a, c.rng.gaussian());

            c.pink_phase += speed * 8.0 * dt;
            while c.pink_phase >= 1.0 {
                c.pink_phase -= 1.0;
                let target = c.pink.tick(&mut c.rng);
                let alpha = 1.0 - (-speed * 8.0 * dt).exp();
                c.pink_smooth += (target - c.pink_smooth) * alpha.clamp(0.0, 1.0);
            }

            c.perlin_t += speed * dt;
            let smooth = perlin_fbm(c.perlin_t, c.seed);

            let mixed = (brown * p.brown + c.pink_smooth * p.pink + smooth * p.perlin) * norm;
            out[i] = soft_limit(mixed * 0.7);
        }
        out
    }
}

/// Rational tanh-like limiter: x / (1 + |x|).
///
/// Drift is Gaussian, so rare excursions beyond 3 sigma exist. A hard clip would
/// glue the parameter to the rail for a while; this curve is monotone, has
/// slope 1 at zero (so normal behaviour is untouched) and approaches +/-1
/// asymptotically, so extreme moments just lean into the limit gracefully.
#[inline]
fn soft_limit(x: f32) -> f32 {
    x / (1.0 + x.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u32, params: OrganicParams, blocks: usize) -> Vec<[f32; CHANNELS]> {
        let mut m = OrganicModulator::new(seed);
        m.prepare(48000.0);
        (0..blocks).map(|_| m.process_block(32, &params)).collect()
    }

    #[test]
    fn bounded_finite_and_roughly_centered() {
        let out = run(7, OrganicParams::default(), 300_000);
        for ch in 0..CHANNELS {
            let mean = out.iter().map(|v| v[ch]).sum::<f32>() / out.len() as f32;
            assert!(out.iter().all(|v| v[ch].is_finite() && v[ch].abs() <= 1.0));
            assert!(mean.abs() < 0.25, "channel {ch} mean {mean}");
        }
    }

    #[test]
    fn deterministic_and_reset_reproduces() {
        let a = run(3, OrganicParams::default(), 2000);
        let b = run(3, OrganicParams::default(), 2000);
        assert!(a == b);
        let mut m = OrganicModulator::new(3);
        m.prepare(48000.0);
        let first = m.process_block(32, &OrganicParams::default());
        for _ in 0..500 {
            m.process_block(32, &OrganicParams::default());
        }
        m.reset();
        assert!(first == m.process_block(32, &OrganicParams::default()));
    }

    #[test]
    fn channels_are_decorrelated() {
        let out = run(11, OrganicParams::default(), 200_000);
        let n = out.len() as f32;
        let corr = |x: usize, y: usize| {
            let mx = out.iter().map(|v| v[x]).sum::<f32>() / n;
            let my = out.iter().map(|v| v[y]).sum::<f32>() / n;
            let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
            for v in &out {
                sxy += (v[x] - mx) * (v[y] - my);
                sxx += (v[x] - mx).powi(2);
                syy += (v[y] - my).powi(2);
            }
            sxy / (sxx * syy).sqrt()
        };
        for (x, y) in [(0, 1), (0, 2), (1, 3), (2, 3)] {
            assert!(corr(x, y).abs() < 0.35, "corr({x},{y}) = {}", corr(x, y));
        }
    }

    #[test]
    fn drift_is_smooth_not_white() {
        let out = run(5, OrganicParams::default(), 100_000);
        for ch in 0..CHANNELS {
            let mean = out.iter().map(|v| v[ch]).sum::<f32>() / out.len() as f32;
            let var = out.iter().map(|v| (v[ch] - mean).powi(2)).sum::<f32>() / out.len() as f32;
            let dvar = out
                .windows(2)
                .map(|w| (w[1][ch] - w[0][ch]).powi(2))
                .sum::<f32>()
                / (out.len() - 1) as f32;
            assert!(dvar / var < 0.01, "channel {ch} diff/var {}", dvar / var);
        }
    }

    #[test]
    fn rate_scales_speed() {
        let slow = run(2, OrganicParams { rate_hz: 0.05, ..Default::default() }, 100_000);
        let fast = run(2, OrganicParams { rate_hz: 1.0, ..Default::default() }, 100_000);
        let roughness = |o: &Vec<[f32; CHANNELS]>| {
            o.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).sum::<f32>()
        };
        assert!(roughness(&fast) > 3.0 * roughness(&slow));
    }
}
