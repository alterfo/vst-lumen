#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
pub struct AdsrSettings {
    attack_inc: f32,
    decay_coef: f32,
    sustain: f32,
    release_coef: f32,
}

fn coef(time: f32, sr: f32) -> f32 {
    1.0 - (-4.6 / (time.max(1e-4) * sr)).exp()
}

impl AdsrSettings {
    pub fn new(attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) -> Self {
        Self {
            attack_inc: 1.0 / (attack.max(1e-4) * sr),
            decay_coef: coef(decay, sr),
            sustain,
            release_coef: coef(release, sr),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Adsr {
    stage: Stage,
    pub value: f32,
}

impl Adsr {
    pub const fn new() -> Self {
        Self {
            stage: Stage::Idle,
            value: 0.0,
        }
    }

    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn release(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    pub fn is_releasing(&self) -> bool {
        self.stage == Stage::Release
    }

    pub fn kill(&mut self) {
        self.stage = Stage::Idle;
        self.value = 0.0;
    }

    #[inline]
    pub fn next(&mut self, s: &AdsrSettings) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.value += s.attack_inc;
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.value += (s.sustain - self.value) * s.decay_coef;
                if (self.value - s.sustain).abs() < 1e-4 {
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                self.value += (s.sustain - self.value) * s.decay_coef;
                if s.sustain < 1e-4 && self.value < 1e-4 {
                    self.kill();
                }
            }
            Stage::Release => {
                self.value -= self.value * s.release_coef;
                if self.value < 1e-4 {
                    self.kill();
                }
            }
        }
        self.value
    }
}
