# Architecture

Crate `lumen` (cdylib + lib), built with nih-plug (CLAP, VST3) and nih_plug_egui. One bundle exports two plugins: the `Lumen` synth and the `Lumen Strings` effect.

| Module | Role |
|---|---|
| `lib.rs` | Plugin, voice allocation (poly/mono/legato, glide), block loop (32 samples), signal chain, `Shared` state for the GUI |
| `params.rs` | All parameters; every knob is a continuous `FloatParam` (enums only for selectors) |
| `wavetable.rs` | 64 frames x 11 mip levels, built by IFFT at startup, never rebuilt on the audio thread |
| `voice.rs` | Oscillators (fractional unison), filter, envelopes, per-note variation, resonator hook |
| `filter.rs` | TPT state-variable filter with continuous LP-BP-HP morph and soft saturation |
| `resonator.rs` | Per-voice waveguide pluck and modal string/membrane/plate |
| `body.rs` | Wooden body resonance bus (10 fixed modes, stereo detune) |
| `granular.rs` | Live granulator on a ring buffer: freeze, reverse, harmonic pitch intervals |
| `fx.rs` | Chorus, ping-pong delay, 8-line FDN reverb |
| `chaos.rs` | Organic drift: Ornstein-Uhlenbeck + Voss-McCartney pink + Perlin; fixed arrays only |
| `evolver.rs` | Catmull-Rom random walk used by Evolve |
| `midi.rs` | Pure MIDI logic: encoder decoding, speed-aware step curve, 14-bit pickup, learn detection, spring inertia |
| `bridge.rs` | Connects the MIDI engine to parameters; lock-free state mirror for the GUI; persisted mappings |
| `gui.rs` | Synth egui editor and MIDI menus (knob physics lives in `widgets.rs`) |
| `presets.rs` | Factory presets and curated Randomize |
| `strings.rs` | `LumenStrings` effect plugin: parameters, input stage (drive, touch, skin), chain, shared meters |
| `sympathetic.rs` | 16 tuned feedback-comb strings with scale tables, plus a Sub Bass bank of 8 strings 1-3 octaves lower fed through a pitch-down shifter |
| `strings_gui.rs` | Effect editor built from `widgets.rs` |
| `strings_presets.rs` | Effect factory presets and preset list with persistent deletion (`~/Library/Application Support/Lumen/strings_deleted_presets.txt`) |
| `resonator/` | Separate workspace crate `lumen-resonator`: resonator-only plugin sharing `sympathetic.rs`, `filter.rs`, `widgets.rs` via `#[path]`; own params, GUI, presets and 100-preset `bank.rs` |
| `strings_bank.rs` | 100 generated presets (Cup Bass family) |
| `widgets.rs` | Shared GUI theme, analog knob, sections, selectors, meters |
| `packgen/mod.rs` | Offline sample-kit driver: kit manifest, WAV writer, background job with progress, CLI analysis |
| `packgen/kit.rs` | The 30 kit voices as parameter tables (kicks, snares, hats, percussion, basses, pads, arps, keys, textures) |
| `packgen/engine.rs` | Offline render engine: `Patch` (plain knob values) driving the same `voice`/`fx`/`body`/`granular` DSP without nih-plug parameters |
| `packgen/groove.rs` | 128 BPM / A phrygian demo groove patterns, 16th grid, one stem per layer |
| `packgen/variant.rs` | Seeded variant generator: archetype (voice pool + fx balance), 16-bar section form, tempo, key, swing, chord walk, per-section patterns, parameter jitter, automation lanes |
| `packgen/score.rs` | `--score` contract: renders an external JSON score (layers, notes, automation) into stems and a mix for other programs |
| `packgen/synth.rs` | Event scheduler that renders voices and multi-event layers sample-accurately |
| `packgen/wav.rs` | 24-bit stereo WAV writer, tail trim, edge fades, peak normalization, spectrum stats |
| `src/bin/lumen_kit.rs` | `lumen-kit` CLI: render the pack or analyze an existing folder |

## Real-time rules

No allocation, locks or blocking calls in `process`. Buffers are allocated in `initialize`. The GUI talks to the
audio thread only through atomics (`Shared`, `MidiShared`); persisted MIDI mappings use `try_lock` on the audio side.

## Sample kit rendering

`packgen` renders audio offline, outside the audio thread: the GUI button and the `lumen-kit` binary
spawn a background thread, build their own `Engine`, write WAVs, and report progress through atomics.
The engine reads a `Patch` struct instead of nih-plug parameters (parameters cannot be written without a
host `GuiContext`), but feeds the same `voice`, `filter`, `resonator`, `body`, `granular`, `fx` and
`chaos` modules, so rendered samples match what the plugin plays.

## MIDI to parameter path

CC event -> `Bridge::on_cc` -> `Engine` (f64 accumulator, speed estimate from sample-accurate timestamps) ->
critically damped spring per block -> `Bridge::eff` replaces the host value of a mapped `FloatParam` until the host
value changes. The host parameter itself is not written from the audio thread.
