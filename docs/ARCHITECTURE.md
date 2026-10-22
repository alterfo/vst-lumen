# Architecture

Crate `lumen` (cdylib + lib), built with nih-plug (CLAP, VST3) and nih_plug_egui.

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
| `gui.rs` | egui editor, knob physics (backlash + spring-mass + fling), MIDI menus |
| `presets.rs` | Factory presets and curated Randomize |

## Real-time rules

No allocation, locks or blocking calls in `process`. Buffers are allocated in `initialize`. The GUI talks to the
audio thread only through atomics (`Shared`, `MidiShared`); persisted MIDI mappings use `try_lock` on the audio side.

## MIDI to parameter path

CC event -> `Bridge::on_cc` -> `Engine` (f64 accumulator, speed estimate from sample-accurate timestamps) ->
critically damped spring per block -> `Bridge::eff` replaces the host value of a mapped `FloatParam` until the host
value changes. The host parameter itself is not written from the audio thread.
