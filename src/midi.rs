use serde::{Deserialize, Serialize};

pub const MAX_SLOTS: usize = 128;
pub const MAX_MAPS: usize = 64;
const DETECT: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Encoding {
    Speed,
    TwosComplement,
    BinaryOffset,
    SignMagnitude,
    Absolute14,
}

impl Encoding {
    pub const ALL: [Encoding; 5] = [
        Encoding::Speed,
        Encoding::TwosComplement,
        Encoding::BinaryOffset,
        Encoding::SignMagnitude,
        Encoding::Absolute14,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Encoding::Speed => "Pot / speed",
            Encoding::TwosComplement => "Encoder 2's compl.",
            Encoding::BinaryOffset => "Encoder offset 64",
            Encoding::SignMagnitude => "Encoder sign-mag",
            Encoding::Absolute14 => "14-bit absolute",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedMap {
    pub id: String,
    pub cc: u8,
    pub channel: u8,
    pub encoding: Encoding,
    pub sensitivity: f32,
    pub invert: bool,
}

#[derive(Clone, Copy)]
pub struct Mapping {
    pub slot: usize,
    pub cc: u8,
    pub channel: u8,
    pub encoding: Encoding,
    pub sensitivity: f32,
    pub invert: bool,
}

#[derive(Clone, Copy)]
pub struct Tuning {
    pub fine: f64,
    pub coarse: f64,
    pub speed_lo: f64,
    pub speed_hi: f64,
    pub inertia_ms: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            fine: 1e-8,
            coarse: 1.0 / 24.0,
            speed_lo: 2.0,
            speed_hi: 90.0,
            inertia_ms: 90.0,
        }
    }
}

pub fn step_per_tick(speed: f64, t: &Tuning) -> f64 {
    let fine = t.fine.max(1e-12);
    let coarse = t.coarse.max(fine);
    let span = (1.0 + t.speed_hi / t.speed_lo).ln().max(1e-9);
    let x = ((1.0 + speed.max(0.0) / t.speed_lo).ln() / span).clamp(0.0, 1.0);
    fine * (coarse / fine).powf(x)
}

pub fn decode_relative(enc: Encoding, raw: i32) -> i32 {
    match enc {
        Encoding::TwosComplement => {
            if raw < 64 {
                raw
            } else {
                raw - 128
            }
        }
        Encoding::BinaryOffset => raw - 64,
        Encoding::SignMagnitude => {
            let mag = raw & 0x3F;
            if raw & 0x40 != 0 {
                -mag
            } else {
                mag
            }
        }
        _ => 0,
    }
}

pub fn detect_encoding(samples: &[i32]) -> Encoding {
    if samples.len() < 4 {
        return Encoding::Speed;
    }
    let mut distinct = [-1i32; DETECT];
    let mut n_distinct = 0;
    for &s in samples {
        if !distinct[..n_distinct].contains(&s) {
            if n_distinct < DETECT {
                distinct[n_distinct] = s;
                n_distinct += 1;
            }
        }
    }
    let mut run = 1;
    let mut longest = 1;
    for w in samples.windows(2) {
        if w[1] == w[0] + 1 || w[1] == w[0] - 1 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
    }
    let monotone_pot = longest >= 4;
    if monotone_pot {
        return Encoding::Speed;
    }
    let near_zero = |v: i32| (1..=6).contains(&v) || (122..=127).contains(&v);
    if samples.iter().all(|&v| near_zero(v)) && n_distinct <= 4 {
        return Encoding::TwosComplement;
    }
    let near_64 = |v: i32| (58..=70).contains(&v) && v != 64;
    if samples.iter().all(|&v| near_64(v)) && n_distinct <= 5 {
        return Encoding::BinaryOffset;
    }
    let sign_mag = |v: i32| (1..=6).contains(&v) || (65..=70).contains(&v);
    if samples.iter().all(|&v| sign_mag(v)) && samples.iter().any(|&v| v >= 65) && n_distinct <= 4 {
        return Encoding::SignMagnitude;
    }
    Encoding::Speed
}

#[derive(Clone, Copy)]
struct Track {
    last_raw: i32,
    last_time: f64,
    speed: f64,
    last_dir: i32,
    msb: i32,
    lsb: i32,
    picked_up: bool,
    detecting: bool,
    detect: [i32; DETECT],
    detect_n: usize,
}

impl Track {
    const fn new() -> Self {
        Self {
            last_raw: -1,
            last_time: f64::NEG_INFINITY,
            speed: 0.0,
            last_dir: 0,
            msb: -1,
            lsb: 0,
            picked_up: false,
            detecting: false,
            detect: [0; DETECT],
            detect_n: 0,
        }
    }
}

pub struct Engine {
    pub tuning: Tuning,
    maps: [Option<Mapping>; MAX_MAPS],
    tracks: [Track; MAX_MAPS],
    target: [f64; MAX_SLOTS],
    current: [f64; MAX_SLOTS],
    velocity: [f64; MAX_SLOTS],
    owned: [bool; MAX_SLOTS],
    dirty: bool,
    host_seen: [f32; MAX_SLOTS],
    sample_rate: f64,
}

impl Engine {
    pub const fn new() -> Self {
        Self {
            tuning: Tuning {
                fine: 1e-8,
                coarse: 1.0 / 24.0,
                speed_lo: 2.0,
                speed_hi: 90.0,
                inertia_ms: 90.0,
            },
            maps: [None; MAX_MAPS],
            tracks: [Track::new(); MAX_MAPS],
            target: [0.0; MAX_SLOTS],
            current: [0.0; MAX_SLOTS],
            velocity: [0.0; MAX_SLOTS],
            owned: [false; MAX_SLOTS],
            dirty: false,
            host_seen: [0.0; MAX_SLOTS],
            sample_rate: 44100.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.reset_motion();
    }

    pub fn reset_motion(&mut self) {
        self.tracks = [Track::new(); MAX_MAPS];
        self.owned = [false; MAX_SLOTS];
        self.velocity = [0.0; MAX_SLOTS];
    }

    pub fn clear_maps(&mut self) {
        self.maps = [None; MAX_MAPS];
        self.reset_motion();
        self.dirty = true;
    }

    pub fn set_map(&mut self, index: usize, m: Option<Mapping>) {
        if index < MAX_MAPS {
            self.dirty = true;
            self.maps[index] = m;
            self.tracks[index] = Track::new();
        }
    }

    pub fn map(&self, index: usize) -> Option<Mapping> {
        self.maps.get(index).copied().flatten()
    }

    pub fn find_free(&self) -> Option<usize> {
        self.maps.iter().position(|m| m.is_none())
    }

    pub fn owned(&self, slot: usize) -> bool {
        self.owned[slot]
    }

    pub fn value(&self, slot: usize) -> f64 {
        self.current[slot]
    }

    pub fn observe_host(&mut self, slot: usize, host_norm: f32) {
        if self.owned[slot] && (host_norm - self.host_seen[slot]).abs() > 1e-6 {
            self.owned[slot] = false;
            self.velocity[slot] = 0.0;
        }
    }

    pub fn begin_learn(&mut self, index: usize, slot: usize, cc: u8, channel: u8) {
        self.maps[index] = Some(Mapping {
            slot,
            cc,
            channel,
            encoding: Encoding::Speed,
            sensitivity: 1.0,
            invert: false,
        });
        let mut t = Track::new();
        t.detecting = true;
        self.tracks[index] = t;
        self.dirty = true;
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    fn take_ownership(&mut self, slot: usize, host_norm: f32) {
        if !self.owned[slot] {
            self.owned[slot] = true;
            self.target[slot] = host_norm as f64;
            self.current[slot] = host_norm as f64;
            self.velocity[slot] = 0.0;
            self.host_seen[slot] = host_norm;
        }
    }

    pub fn on_cc(
        &mut self,
        time_samples: f64,
        channel: u8,
        cc: u8,
        raw: i32,
        host_norm: &dyn Fn(usize) -> f32,
    ) {
        for i in 0..MAX_MAPS {
            let m = match self.maps[i] {
                Some(m) => m,
                None => continue,
            };
            let pair_lsb = m.encoding == Encoding::Absolute14 && cc == m.cc.wrapping_add(32);
            if (cc != m.cc && !pair_lsb) || (m.channel != 255 && m.channel != channel) {
                continue;
            }
            self.apply(i, m, time_samples, cc, raw, host_norm);
        }
    }

    fn apply(
        &mut self,
        i: usize,
        mut m: Mapping,
        time: f64,
        cc: u8,
        raw: i32,
        host_norm: &dyn Fn(usize) -> f32,
    ) {
        let mut tr = self.tracks[i];

        if tr.detecting {
            if tr.detect_n < DETECT {
                tr.detect[tr.detect_n] = raw;
                tr.detect_n += 1;
            }
            if tr.detect_n >= DETECT {
                m.encoding = detect_encoding(&tr.detect);
                self.maps[i] = Some(m);
                self.dirty = true;
                tr.detecting = false;
                tr.last_raw = -1;
            }
        }

        let slot = m.slot;
        let sign = if m.invert { -1.0 } else { 1.0 };
        let mut ticks = 0i32;
        let mut absolute: Option<f64> = None;

        match m.encoding {
            Encoding::Speed => {
                if tr.last_raw >= 0 {
                    ticks = raw - tr.last_raw;
                }
                tr.last_raw = raw;
            }
            Encoding::Absolute14 => {
                if cc == m.cc {
                    tr.msb = raw;
                } else {
                    tr.lsb = raw;
                }
                if tr.msb >= 0 {
                    absolute = Some(((tr.msb << 7) | tr.lsb) as f64 / 16383.0);
                }
            }
            enc => ticks = decode_relative(enc, raw),
        }

        if let Some(v) = absolute {
            let host = host_norm(slot);
            self.take_ownership_if_close(slot, host, v, &mut tr);
            if tr.picked_up {
                self.target[slot] = if m.invert { 1.0 - v } else { v };
            }
            self.tracks[i] = tr;
            return;
        }

        if ticks == 0 {
            self.tracks[i] = tr;
            return;
        }

        let dt = (time - tr.last_time) / self.sample_rate;
        let dir = ticks.signum();
        if dt > 0.35 || tr.last_time == f64::NEG_INFINITY {
            tr.speed = 0.0;
        } else {
            let inst = ticks.abs() as f64 / dt.max(0.001);
            tr.speed += (inst - tr.speed) * 0.45;
            if dir != tr.last_dir && tr.last_dir != 0 {
                tr.speed *= 0.3;
            }
        }
        tr.last_time = time;
        tr.last_dir = dir;

        self.take_ownership(slot, host_norm(slot));
        let step = step_per_tick(tr.speed, &self.tuning) * m.sensitivity as f64;
        let delta = ticks as f64 * step * sign;
        self.target[slot] = (self.target[slot] + delta).clamp(0.0, 1.0);
        self.tracks[i] = tr;
    }

    fn take_ownership_if_close(&mut self, slot: usize, host: f32, v: f64, tr: &mut Track) {
        if !tr.picked_up {
            let reference = if self.owned[slot] { self.target[slot] } else { host as f64 };
            if (v - reference).abs() < 0.02 {
                tr.picked_up = true;
                self.take_ownership(slot, host);
            }
        } else {
            self.take_ownership(slot, host);
        }
    }

    pub fn advance(&mut self, frames: usize) {
        let dt = frames as f64 / self.sample_rate;
        let tau = (self.tuning.inertia_ms * 0.001).max(1e-4);
        let w = 1.0 / tau;
        for s in 0..MAX_SLOTS {
            if !self.owned[s] {
                continue;
            }
            let steps = 4;
            let h = dt / steps as f64;
            for _ in 0..steps {
                let acc = w * w * (self.target[s] - self.current[s]) - 2.0 * w * self.velocity[s];
                self.velocity[s] += acc * h;
                self.current[s] += self.velocity[s] * h;
            }
            if self.current[s] < 0.0 {
                self.current[s] = 0.0;
                self.velocity[s] = 0.0;
            } else if self.current[s] > 1.0 {
                self.current[s] = 1.0;
                self.velocity[s] = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn engine_with(enc: Encoding) -> Engine {
        let mut e = Engine::new();
        e.prepare(SR);
        e.set_map(
            0,
            Some(Mapping {
                slot: 0,
                cc: 20,
                channel: 255,
                encoding: enc,
                sensitivity: 1.0,
                invert: false,
            }),
        );
        e
    }

    fn settle(e: &mut Engine) {
        for _ in 0..4000 {
            e.advance(32);
        }
    }

    #[test]
    fn step_grows_monotonically_with_speed() {
        let t = Tuning::default();
        let mut prev = 0.0;
        for v in [0.0, 1.0, 2.0, 5.0, 20.0, 60.0, 120.0, 400.0] {
            let s = step_per_tick(v, &t);
            assert!(s >= prev);
            prev = s;
        }
        assert!((step_per_tick(0.0, &t) - t.fine).abs() < 1e-12);
        assert!(step_per_tick(1e6, &t) <= t.coarse * 1.0001);
        assert!(step_per_tick(1e6, &t) / step_per_tick(0.0, &t) > 1000.0);
    }

    #[test]
    fn slow_turn_is_millions_of_times_finer_than_7_bit() {
        let mut e = engine_with(Encoding::TwosComplement);
        let host = |_s: usize| 0.5f32;
        let mut t = 0.0;
        for _ in 0..10 {
            e.on_cc(t, 0, 20, 1, &host);
            t += SR * 1.0;
        }
        settle(&mut e);
        let moved = e.value(0) - 0.5;
        assert!(moved > 0.0 && moved < 1e-5, "moved {moved}");
        let one_tick = moved / 10.0;
        let seven_bit = 1.0 / 127.0;
        assert!(seven_bit / one_tick > 100_000.0, "ratio {}", seven_bit / one_tick);
    }

    #[test]
    fn fast_flick_sweeps_a_big_range() {
        let mut e = engine_with(Encoding::TwosComplement);
        let host = |_s: usize| 0.1f32;
        let mut t = 0.0;
        for _ in 0..30 {
            e.on_cc(t, 0, 20, 1, &host);
            t += SR * 0.004;
        }
        settle(&mut e);
        assert!(e.value(0) > 0.6, "value {}", e.value(0));
    }

    #[test]
    fn absolute_pot_uses_speed_not_position() {
        let mut e = engine_with(Encoding::Speed);
        let host = |_s: usize| 0.5f32;
        let mut t = 0.0;
        for raw in 60..70 {
            e.on_cc(t, 0, 20, raw, &host);
            t += SR * 0.5;
        }
        settle(&mut e);
        assert!((e.value(0) - 0.5).abs() < 1e-3, "slow pot drift {}", e.value(0) - 0.5);
        let mut t = 0.0;
        for raw in 70..110 {
            e.on_cc(t, 0, 20, raw, &host);
            t += SR * 0.004;
        }
        settle(&mut e);
        assert!(e.value(0) > 0.7, "fast pot sweep {}", e.value(0));
    }

    #[test]
    fn relative_decodings() {
        assert_eq!(decode_relative(Encoding::TwosComplement, 3), 3);
        assert_eq!(decode_relative(Encoding::TwosComplement, 125), -3);
        assert_eq!(decode_relative(Encoding::BinaryOffset, 66), 2);
        assert_eq!(decode_relative(Encoding::BinaryOffset, 62), -2);
        assert_eq!(decode_relative(Encoding::SignMagnitude, 0x41), -1);
        assert_eq!(decode_relative(Encoding::SignMagnitude, 0x03), 3);
    }

    #[test]
    fn direction_and_invert() {
        let host = |_s: usize| 0.5f32;
        let mut e = engine_with(Encoding::TwosComplement);
        let mut t = 0.0;
        for _ in 0..10 {
            e.on_cc(t, 0, 20, 127, &host);
            t += SR * 0.02;
        }
        settle(&mut e);
        assert!(e.value(0) < 0.5);
        let mut e = engine_with(Encoding::TwosComplement);
        let mut m = e.map(0).unwrap();
        m.invert = true;
        e.set_map(0, Some(m));
        let mut t = 0.0;
        for _ in 0..10 {
            e.on_cc(t, 0, 20, 127, &host);
            t += SR * 0.02;
        }
        settle(&mut e);
        assert!(e.value(0) > 0.5);
    }

    #[test]
    fn absolute_14_bit_pickup_and_resolution() {
        let mut e = engine_with(Encoding::Absolute14);
        let host = |_s: usize| 0.30f32;
        e.on_cc(0.0, 0, 20, 100, &host);
        e.on_cc(0.0, 0, 52, 0, &host);
        settle(&mut e);
        assert!(!e.owned(0), "must not jump: pickup");
        e.on_cc(1.0, 0, 20, 38, &host);
        e.on_cc(1.0, 0, 52, 5, &host);
        settle(&mut e);
        assert!(e.owned(0));
        let expected = ((38 << 7) | 5) as f64 / 16383.0;
        assert!((e.value(0) - expected).abs() < 1e-4);
        e.on_cc(2.0, 0, 52, 6, &host);
        settle(&mut e);
        let d = e.value(0) - expected;
        assert!(d > 0.0 && d < 1.0 / 16000.0);
    }

    #[test]
    fn host_change_releases_ownership() {
        let mut e = engine_with(Encoding::TwosComplement);
        e.on_cc(0.0, 0, 20, 1, &|_| 0.5);
        assert!(e.owned(0));
        e.observe_host(0, 0.5);
        assert!(e.owned(0));
        e.observe_host(0, 0.62);
        assert!(!e.owned(0));
    }

    #[test]
    fn learn_detects_encoders_and_pots() {
        assert_eq!(detect_encoding(&[1, 1, 1, 2, 1, 1]), Encoding::TwosComplement);
        assert_eq!(detect_encoding(&[127, 127, 126, 127, 127, 127]), Encoding::TwosComplement);
        assert_eq!(detect_encoding(&[65, 66, 65, 65, 66, 65]), Encoding::BinaryOffset);
        assert_eq!(detect_encoding(&[1, 2, 3, 4, 5, 6]), Encoding::Speed);
        assert_eq!(detect_encoding(&[40, 41, 43, 44, 47, 52]), Encoding::Speed);
    }

    #[test]
    fn inertia_is_smooth_without_overshoot() {
        let mut e = engine_with(Encoding::TwosComplement);
        let host = |_s: usize| 0.5f32;
        let mut t = 0.0;
        for _ in 0..20 {
            e.on_cc(t, 0, 20, 1, &host);
            t += SR * 0.002;
        }
        let mut prev = e.value(0);
        let mut max_jump = 0.0f64;
        for _ in 0..3000 {
            e.advance(32);
            let v = e.value(0);
            max_jump = max_jump.max((v - prev).abs());
            assert!(v >= prev - 1e-12);
            prev = v;
        }
        assert!(max_jump < 0.01, "max per-block jump {max_jump}");
        assert!(prev <= e.target[0] + 1e-9);
    }
}
