pub struct Evolver {
    phase: f32,
    points: [f32; 4],
    rng: u32,
}

impl Evolver {
    pub fn new(seed: u32) -> Self {
        let mut e = Self {
            phase: 0.0,
            points: [0.0; 4],
            rng: seed | 1,
        };
        for i in 0..4 {
            e.points[i] = e.rand();
        }
        e
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 8388608.0 - 1.0
    }

    pub fn advance(&mut self, inc: f32) -> f32 {
        self.phase += inc;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.points.rotate_left(1);
            self.points[3] = self.rand();
        }
        let t = self.phase;
        let [p0, p1, p2, p3] = self.points;
        let v = 0.5
            * ((2.0 * p1)
                + (-p0 + p2) * t
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t);
        v.clamp(-1.0, 1.0)
    }
}

