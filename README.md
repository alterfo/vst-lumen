# Lumen

Free wavetable synthesizer (CLAP + VST3) for organic downtempo and ambient, written in Rust with nih-plug.

## Build

```
cargo xtask bundle lumen --release
cp -R target/bundled/lumen.vst3 ~/Library/Audio/Plug-Ins/VST3/
cp -R target/bundled/lumen.clap ~/Library/Audio/Plug-Ins/CLAP/
```

Standalone (for development):
`cargo standalone` (alias for `cargo run --release --features standalone --bin lumen-standalone --`).
Without the `--features standalone` flag the bin target does not build, so use the alias.
To keep the window opening reliably, pass a backend or device if you have JACK installed:
`cargo standalone -- -b core-audio`.

## Signal path

Two wavetable oscillators (mip-mapped, fractional unison, continuous morph) + sub + noise
-> morphing SVF filter (LP-BP-HP) -> per-voice resonator (waveguide pluck, modal string, membrane, plate)
-> wooden body bus -> granular cloud (freeze, harmonic intervals) -> chorus -> ping-pong delay -> FDN reverb.

## Organic behaviour

- Organic macro: per-note random pitch/cutoff/table/level variation, slow per-voice drift.
- Chaos macro: three-layer drift (Ornstein-Uhlenbeck, 1/f pink, Perlin) modulating table, morph, cutoff, pitch and grain position/size.
- Evolve: autonomous smooth random walk of table position, morph and cutoff.

## Controls

All knobs are continuous. The GUI knob has free play (backlash) and inertia (spring-mass with fling).
Drag: normal; Shift: fine; Alt/Cmd: ultra-fine; scroll: nudge; double-click: reset; right-click: MIDI learn.

## MIDI

Right-click a knob, choose "MIDI learn", move a controller. Encoder type is auto-detected from the first messages
(absolute pot, two's complement, offset-64, sign-magnitude); 14-bit CC pairs (CC n + n+32) are supported with pickup.
Turn speed sets the step per tick on a logarithmic curve from `MIDI Fine Step` (default 1e-8 of the range) to
`1/MIDI Coarse Ticks`; values accumulate in f64 and move through a critically damped spring (`MIDI Inertia`).
A mapped parameter follows MIDI until the host or GUI changes it.

## Lumen Strings (effect)

The same bundle also contains **Lumen Strings**, an audio effect for electric guitar and erhu (stereo in, stereo out).

Input -> high-pass -> ADAA tanh drive + tone -> envelope-driven touch filter -> erhu "skin" formants (two band-pass peaks
around the python-skin resonance) -> 16 sympathetic strings tuned to a root and a scale (Gong, Shang, Jue, Zhi, Yu,
natural minor, Dorian, Phrygian, fifths) -> wooden body bus -> granular cloud -> chorus -> delay -> FDN reverb -> Dry/Wet.

- Sympathetic strings: tuned feedback combs with fractional delay; played notes make the matching strings ring.
- Sub Bass: a second bank of 8 strings 1-3 octaves below the root (Sub Drop), excited by the input pitch-shifted down, so a high ping (porcelain cup) rings as a bass.
- Presets: the "x" button next to the preset selector deletes the current preset permanently (stored in `~/Library/Application Support/Lumen/strings_deleted_presets.txt`; delete that file to restore all).
- Bloom: the wet layers duck while you play and swell back in the gaps.
- Organic: slow chaos drift of string tuning, body size and grain position/size.
- The knobs and presets share the analog knob physics of the synth.

## Lumen Resonator (separate plugin)

A standalone VST3/CLAP in `resonator/` with only the sympathetic strings and the Sub Bass bank, 100 presets and preset deletion ("x" button; stored in `~/Library/Application Support/Lumen/resonator_deleted_presets.txt`).
It reuses `sympathetic.rs`, `filter.rs` and `widgets.rs` from the main crate via `#[path]`.

```
cargo xtask bundle lumen-resonator --release
cp -R target/bundled/lumen-resonator.vst3 ~/Library/Audio/Plug-Ins/VST3/
cp -R target/bundled/lumen-resonator.clap ~/Library/Audio/Plug-Ins/CLAP/
```

## Sample kit generator

The plugin can also write its own sample pack: 30 voices (kicks, snares, hats, percussion, basses,
pads, arps, keys, textures) plus a layered demo groove, rendered offline at 48 kHz / 24-bit stereo.

- In the plugin: **Output → Sample kit → One-shots / Demo groove / Everything**. Pick the folder in the
  window; the render runs on a background thread and never touches the audio thread.
- From a terminal, without a DAW:

```
cargo run --release --bin lumen-kit -- --mode all --out "$HOME/Music Projects/Samples/lumen-kit"
cargo run --release --bin lumen-kit -- --analyze "$HOME/Music Projects/Samples/lumen-kit/one-shots"
```

Output layout:

```
one-shots/  01-kick-deep.wav … 30-texture-downlifter.wav, kit.json, README.md
groove/     kick.wav snare.wav hats.wav hat-open.wav shaker.wav perc.wav bass.wav pad.wav arp.wav
            mix.wav (same stems summed, −3 dBFS), groove.json, README.md
```

Every file is peak-normalized per voice type, fades are applied at the edges, and every groove stem
starts on bar 1 at 128 BPM in A phrygian on a 16th grid, so stems stack at 0.0 without alignment work.
The kit tables live in `src/packgen/kit.rs`, the groove patterns in `src/packgen/groove.rs`.

### Mix batches

`--mixes N` renders N *different* mixes from consecutive seeds. Each seed picks an **archetype**
(dub techno / ambient drift / trip hop / broken beat / cinematic / lo-fi house) that sets the voice
pool and the effect balance, then builds a **16-bar form** out of sections (intro / main / break / drop /
outro in one of four layouts), tempo (86–134 BPM), key (8 roots x phrygian / dorian / minor / lydian /
pentatonic), swing, an 8-chord progression (one chord per 2 bars), drum patterns per section, a bass line
and a 2-bar motif, per-section density and fills, a parameter jitter across the whole chain (table,
cutoff, resonance, drive, unison/detune, resonator, body, grain, chorus/delay/reverb, envelopes, LFO,
evolve) and 1–3 automation lanes per melodic layer (cutoff, morph, grain, resonator, table, delay,
filter mode). Same seed -> byte-identical audio; different seeds -> different sounds.

```
cargo run --release --bin lumen-kit -- --mixes 100 --seed 1
cargo run --release --bin lumen-kit -- --mixes 100 --seed 1 --stems   # keep the stems per mix
```

Output: `mixes/NNN-seed<seed>/mix.wav` (plus stems with `--stems`), `variant.json` per mix, and
`mixes/index.json` + `mixes/INDEX.md` with BPM, key, archetype, form, voices and automation per mix.

### Rendering an external score

`--score file.json` renders notes coming from another program (used by `polymuse`) into stems and a mix:

```json
{"title": "sketch", "bpm": 96, "bars": 16, "seed": 3, "jitter": 0.5,
 "layers": [
   {"name": "Kick", "kit_voice": "kick-deep", "peak_db": -6,
    "events": [{"time": 0.0, "note": 36, "velocity": 0.95, "duration": 0.02}]},
   {"name": "Harmony", "kit_voice": "pad-warm", "events": [
      {"time": 0.0, "note": 57, "velocity": 90, "duration": 3.7}],
    "automation": [{"target": "cutoff", "from": 0.3, "to": 0.9, "curve": 0.3}]}
 ]}
```

`velocity` accepts both 0–1 and 0–127. Output: `<out>/mix.wav`, `<out>/stems/*.wav`, `<out>/report.json`.
Unknown `kit_voice` fails with the list of valid voices; `kit.json` in the one-shots folder has them.

## Procedural generation

Everything the generator renders comes from one seed, and the seed drives the *music theory*, the
*sound design* and the *mix* at the same time:

1. **Taste** — the batch reads `mixes/index.json` and checks which folders still exist. Deleted
   folders count as rejected mixes; kept ones as approved. A smoothed log-odds score over archetype and
   voice usage rejects candidates that look like the rejected ones, which is why a batch after a cleanup
   drifts toward what survived.
2. **Archetype** — one of six (dub techno, ambient drift, trip hop, broken beat, cinematic, lo-fi house)
   sets the voice pool per role and the effect balance (reverb/delay/chorus/grain), with a 30–45 % chance
   of pulling an extra voice from the wide pool so two mixes rarely share a palette.
3. **Character** — one of eight timbre biases (bright, dark, wet, dry, resonant, gritty, detuned, grainy)
   scales cutoff, resonance, drive, reverb send, unison/detune and grain amount across every layer.
4. **Form** — 16 bars assembled from intro / main / break / drop / outro sections in one of four layouts;
   each section has its own drum density, fill, texture and rest behaviour.
5. **Harmony** — an 8-chord walk (one chord per 2 bars) through the mode, with voicings derived from scale
   degrees; the bass follows the chords with intervals, and a 2-bar motif layer enters in main/drop.
6. **Automation** — 1–3 lanes per melodic layer (cutoff, morph, grain, resonator, table, delay, filter
   mode) run over the whole 16 bars, so the timbre moves instead of repeating.
7. **Diversity guard** — a candidate is only accepted if its voice set differs from every mix already
   rendered (and from the batch itself) by at least four voices.

Same seed -> byte-identical audio, so any mix can be reproduced with `--mixes 1 --seed <n>`. MP3 copies
(320 kbps, ffmpeg) are written next to the WAV with `--mp3`, which cuts a 100-mix batch from ~1.1 GB to
~250 MB.

## Tests

`cargo test` covers wavetable levels, waveguide tuning and decay, resonator stability, granular pitch/freeze,
body response, chaos statistics, MIDI engine behaviour, preset validity and an end-to-end MIDI learn.
