use crate::midi::{Encoding, Engine, Mapping, SavedMap, Tuning, MAX_MAPS, MAX_SLOTS};
use crate::params::LumenParams;
use nih_plug::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;

fn signature(items: impl Iterator<Item = (usize, u8, u8, f32)>) -> u64 {
    let mut sig = 0u64;
    let mut n = 0u64;
    for (id_len, cc, channel, sens) in items {
        sig = fold_sig(sig, id_len, cc, channel, sens);
        n += 1;
    }
    sig ^ (n << 48)
}

fn fold_sig(sig: u64, id_len: usize, cc: u8, channel: u8, sens: f32) -> u64 {
    sig.wrapping_mul(1_000_003)
        .wrapping_add(cc as u64 + 131 * channel as u64 + sens.to_bits() as u64)
        .wrapping_add(id_len as u64 * 7919)
}

pub struct Slots {
    ptrs: Vec<ParamPtr>,
    ids: Vec<String>,
    by_addr: HashMap<usize, usize>,
    by_id: HashMap<String, usize>,
}

impl Slots {
    pub fn new(params: &LumenParams) -> Self {
        let mut s = Self {
            ptrs: Vec::new(),
            ids: Vec::new(),
            by_addr: HashMap::new(),
            by_id: HashMap::new(),
        };
        for (id, ptr, _) in params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                if s.ptrs.len() >= MAX_SLOTS {
                    break;
                }
                let slot = s.ptrs.len();
                s.by_addr.insert(p as usize, slot);
                s.by_id.insert(id.clone(), slot);
                s.ids.push(id);
                s.ptrs.push(ptr);
            }
        }
        s
    }

    pub fn len(&self) -> usize {
        self.ptrs.len()
    }

    pub fn id(&self, slot: usize) -> &str {
        &self.ids[slot]
    }

    pub fn slot_of(&self, p: &FloatParam) -> Option<usize> {
        self.by_addr.get(&(p as *const FloatParam as usize)).copied()
    }

    pub fn slot_of_id(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    pub fn host_norm(&self, slot: usize) -> f32 {
        unsafe { self.ptrs[slot].unmodulated_normalized_value() }
    }
}

pub struct MidiShared {
    pub slots: Slots,
    pub learn: AtomicI32,
    pub version: AtomicU32,
    pub snapshot_seq: AtomicU32,
    pub last_cc: AtomicI32,
    pub owned: Vec<AtomicBool>,
    pub value: Vec<AtomicF32>,
    pub map_slot: Vec<AtomicI32>,
    pub map_bits: Vec<AtomicU32>,
    pub map_sens: Vec<AtomicF32>,
}

fn encode_bits(m: &Mapping) -> u32 {
    let enc = Encoding::ALL.iter().position(|e| *e == m.encoding).unwrap_or(0) as u32;
    m.cc as u32 | (m.channel as u32) << 8 | enc << 16 | (m.invert as u32) << 24
}

fn decode_bits(bits: u32) -> (u8, u8, Encoding, bool) {
    let enc = Encoding::ALL
        .get(((bits >> 16) & 0xFF) as usize)
        .copied()
        .unwrap_or(Encoding::Speed);
    (bits as u8, (bits >> 8) as u8, enc, (bits >> 24) & 1 == 1)
}

impl MidiShared {
    pub fn new(params: &LumenParams) -> Self {
        let slots = Slots::new(params);
        let n = slots.len();
        Self {
            slots,
            learn: AtomicI32::new(0),
            version: AtomicU32::new(0),
            snapshot_seq: AtomicU32::new(0),
            last_cc: AtomicI32::new(-1),
            owned: (0..n).map(|_| AtomicBool::new(false)).collect(),
            value: (0..n).map(|_| AtomicF32::new(0.0)).collect(),
            map_slot: (0..MAX_MAPS).map(|_| AtomicI32::new(-1)).collect(),
            map_bits: (0..MAX_MAPS).map(|_| AtomicU32::new(0)).collect(),
            map_sens: (0..MAX_MAPS).map(|_| AtomicF32::new(1.0)).collect(),
        }
    }

    pub fn snapshot(&self) -> Vec<SavedMap> {
        let mut out = Vec::new();
        for i in 0..MAX_MAPS {
            let slot = self.map_slot[i].load(Ordering::Relaxed);
            if slot < 0 || slot as usize >= self.slots.len() {
                continue;
            }
            let (cc, channel, encoding, invert) = decode_bits(self.map_bits[i].load(Ordering::Relaxed));
            out.push(SavedMap {
                id: self.slots.id(slot as usize).to_string(),
                cc,
                channel,
                encoding,
                sensitivity: self.map_sens[i].load(Ordering::Relaxed),
                invert,
            });
        }
        out
    }

    pub fn request_learn(&self, slot: usize) {
        self.learn.store(slot as i32 + 1, Ordering::Relaxed);
    }

    pub fn cancel_learn(&self) {
        self.learn.store(0, Ordering::Relaxed);
    }

    pub fn commit(&self, params: &LumenParams, maps: Vec<SavedMap>) {
        if let Ok(mut store) = params.midi_maps.lock() {
            *store = maps;
        }
        self.version.fetch_add(1, Ordering::Release);
    }
}

pub struct Bridge {
    engine: Engine,
    shared: Arc<MidiShared>,
    params: Arc<LumenParams>,
    sample_clock: f64,
    loaded_version: u32,
    store_sig: u64,
    blocks: u32,
}

impl Bridge {
    pub fn new(params: Arc<LumenParams>, shared: Arc<MidiShared>) -> Self {
        Self {
            engine: Engine::new(),
            shared,
            params,
            sample_clock: 0.0,
            loaded_version: u32::MAX,
            store_sig: 0,
            blocks: 0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.engine.prepare(sample_rate as f64);
        self.loaded_version = u32::MAX;
    }

    pub fn reset(&mut self) {
        self.engine.reset_motion();
    }

    pub fn advance_clock(&mut self, frames: usize) {
        self.sample_clock += frames as f64;
    }

    pub fn on_cc(&mut self, timing: usize, channel: u8, cc: u8, value: f32) {
        let raw = (value * 127.0).round() as i32;
        let time = self.sample_clock + timing as f64;
        self.shared.last_cc.store(cc as i32, Ordering::Relaxed);

        let learn = self.shared.learn.load(Ordering::Relaxed);
        if learn > 0 && cc < 120 {
            let slot = (learn - 1) as usize;
            let existing = (0..MAX_MAPS).find(|&i| self.engine.map(i).map(|m| m.slot) == Some(slot));
            if let Some(idx) = existing.or_else(|| self.engine.find_free()) {
                self.engine.begin_learn(idx, slot, cc, channel);
            }
            self.shared.learn.store(0, Ordering::Relaxed);
        }

        let slots = &self.shared.slots;
        self.engine.on_cc(time, channel, cc, raw, &|s| slots.host_norm(s));
    }

    fn sync_store(&mut self) {
        let version = self.shared.version.load(Ordering::Acquire);
        let periodic = self.blocks % 512 == 0;
        if version == self.loaded_version && !periodic {
            return;
        }
        if let Ok(store) = self.params.midi_maps.try_lock() {
            let sig = signature(
                store
                    .iter()
                    .filter(|m| self.shared.slots.slot_of_id(&m.id).is_some())
                    .map(|m| (m.id.len(), m.cc, m.channel, m.sensitivity)),
            );
            if version != self.loaded_version || sig != self.store_sig {
                self.engine.clear_maps();
                for (i, m) in store.iter().take(MAX_MAPS).enumerate() {
                    if let Some(slot) = self.shared.slots.slot_of_id(&m.id) {
                        self.engine.set_map(
                            i,
                            Some(Mapping {
                                slot,
                                cc: m.cc,
                                channel: m.channel,
                                encoding: m.encoding,
                                sensitivity: m.sensitivity,
                                invert: m.invert,
                            }),
                        );
                    }
                }
                self.store_sig = sig;
                self.loaded_version = version;
            }
        }
    }

    pub fn pre_block(&mut self, frames: usize) {
        self.blocks = self.blocks.wrapping_add(1);
        self.sync_store();

        let mp = &self.params.midi;
        self.engine.tuning = Tuning {
            fine: 10f64.powf(mp.fine_exp.value() as f64),
            coarse: 1.0 / mp.coarse_div.value().max(1.0) as f64,
            speed_lo: 2.0,
            speed_hi: mp.speed_hi.value() as f64,
            inertia_ms: mp.inertia.value() as f64,
        };

        for i in 0..MAX_MAPS {
            if let Some(m) = self.engine.map(i) {
                self.engine.observe_host(m.slot, self.shared.slots.host_norm(m.slot));
            }
        }
        self.engine.advance(frames);

        if self.engine.take_dirty() {
            for i in 0..MAX_MAPS {
                match self.engine.map(i) {
                    Some(m) => {
                        self.shared.map_slot[i].store(m.slot as i32, Ordering::Relaxed);
                        self.shared.map_bits[i].store(encode_bits(&m), Ordering::Relaxed);
                        self.shared.map_sens[i].store(m.sensitivity, Ordering::Relaxed);
                    }
                    None => self.shared.map_slot[i].store(-1, Ordering::Relaxed),
                }
            }
            let engine = &self.engine;
            let slots = &self.shared.slots;
            self.store_sig = signature(
                (0..MAX_MAPS)
                    .filter_map(|i| engine.map(i))
                    .map(|m| (slots.id(m.slot).len(), m.cc, m.channel, m.sensitivity)),
            );
            self.shared.snapshot_seq.fetch_add(1, Ordering::Release);
        }

        for s in 0..self.shared.slots.len() {
            self.shared.owned[s].store(self.engine.owned(s), Ordering::Relaxed);
            self.shared.value[s].store(self.engine.value(s) as f32, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn eff(&self, p: &FloatParam, n: u32) -> f32 {
        let smoothed = p.smoothed.next_step(n);
        match self.shared.slots.slot_of(p) {
            Some(s) if self.engine.owned(s) => p.preview_plain(self.engine.value(s) as f32),
            _ => smoothed,
        }
    }

    #[inline]
    pub fn val(&self, p: &FloatParam) -> f32 {
        match self.shared.slots.slot_of(p) {
            Some(s) if self.engine.owned(s) => p.preview_plain(self.engine.value(s) as f32),
            _ => p.value(),
        }
    }
}

impl MidiShared {
    pub fn store_only(&self, params: &LumenParams, maps: Vec<SavedMap>) {
        if let Ok(mut store) = params.midi_maps.lock() {
            *store = maps;
        }
    }
}
