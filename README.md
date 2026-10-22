# Lumen

Free wavetable synthesizer (CLAP + VST3) for organic downtempo and ambient, written in Rust with nih-plug.

## Build

```
cargo xtask bundle lumen --release
cp -R target/bundled/lumen.vst3 ~/Library/Audio/Plug-Ins/VST3/
cp -R target/bundled/lumen.clap ~/Library/Audio/Plug-Ins/CLAP/
```

Standalone (for development): `cargo run --release --features standalone --bin lumen-standalone`.

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

## Tests

`cargo test` covers wavetable levels, waveguide tuning and decay, resonator stability, granular pitch/freeze,
body response, chaos statistics, MIDI engine behaviour, preset validity and an end-to-end MIDI learn.
