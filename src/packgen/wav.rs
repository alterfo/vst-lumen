use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy)]
pub struct SampleStats {
    pub seconds: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    pub centroid_hz: f32,
    pub band_low: f32,
    pub band_mid: f32,
    pub band_high: f32,
}

fn amplitude_to_db(a: f32) -> f32 {
    if a <= 1e-9 {
        -120.0
    } else {
        20.0 * a.log10()
    }
}

pub fn trim_tail(left: &mut Vec<f32>, right: &mut Vec<f32>, sr: f32) {
    let n = left.len().min(right.len());
    let mut peak = 0.0f32;
    for i in 0..n {
        peak = peak.max(left[i].abs()).max(right[i].abs());
    }
    let floor = peak * 1e-4;
    let mut last = n;
    while last > 0 {
        let i = last - 1;
        if left[i].abs() > floor || right[i].abs() > floor {
            break;
        }
        last -= 1;
    }
    let keep = (last + (0.02 * sr) as usize).min(n);
    left.truncate(keep);
    right.truncate(keep);
}

pub fn fade_edges(left: &mut [f32], right: &mut [f32], sr: f32) {
    let n = left.len().min(right.len());
    let fin = ((0.002 * sr) as usize).min(n / 2);
    let fout = ((0.008 * sr) as usize).min(n / 2);
    for i in 0..fin {
        let g = i as f32 / fin as f32;
        left[i] *= g;
        right[i] *= g;
    }
    for i in 0..fout {
        let g = i as f32 / fout as f32;
        left[n - 1 - i] *= g;
        right[n - 1 - i] *= g;
    }
}

pub fn normalize(left: &mut [f32], right: &mut [f32], target_peak_db: f32) {
    let mut peak = 0.0f32;
    for i in 0..left.len().min(right.len()) {
        peak = peak.max(left[i].abs()).max(right[i].abs());
    }
    if peak <= 1e-7 {
        return;
    }
    let gain = util::db_to_gain(target_peak_db) / peak;
    for v in left.iter_mut() {
        *v *= gain;
    }
    for v in right.iter_mut() {
        *v *= gain;
    }
}

pub fn stats(left: &[f32], right: &[f32], sr: f32) -> SampleStats {
    let n = left.len().min(right.len());
    let mut peak = 0.0f32;
    let mut sum = 0.0f64;
    for i in 0..n {
        peak = peak.max(left[i].abs()).max(right[i].abs());
        sum += (left[i] * left[i] + right[i] * right[i]) as f64 * 0.5;
    }
    let (centroid, low, mid, high) = spectrum(left, right, sr);
    SampleStats {
        seconds: n as f32 / sr,
        peak_db: amplitude_to_db(peak),
        rms_db: amplitude_to_db((sum / n.max(1) as f64).sqrt() as f32),
        centroid_hz: centroid,
        band_low: low,
        band_mid: mid,
        band_high: high,
    }
}

fn spectrum(left: &[f32], right: &[f32], sr: f32) -> (f32, f32, f32, f32) {
    use realfft::RealFftPlanner;
    let size = 4096usize;
    let n = left.len().min(right.len());
    if n < size {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(size);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let mut acc = vec![0.0f64; output.len()];
    let frames = (n / size).min(24);
    let step = (n / frames).max(1);
    for f in 0..frames {
        let start = (f * step).min(n - size);
        for i in 0..size {
            let x = (left[start + i] + right[start + i]) * 0.5;
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos();
            input[i] = x * w;
        }
        if fft.process(&mut input, &mut output).is_err() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        for (i, bin) in output.iter().enumerate() {
            acc[i] += (bin.re * bin.re + bin.im * bin.im) as f64;
        }
    }
    let bin_hz = sr as f64 / size as f64;
    let mut total = 0.0f64;
    let mut weighted = 0.0f64;
    let mut low = 0.0f64;
    let mut mid = 0.0f64;
    let mut high = 0.0f64;
    for i in 1..acc.len() {
        let hz = i as f64 * bin_hz;
        let mag = acc[i].sqrt();
        total += mag;
        weighted += mag * hz;
        if hz < 200.0 {
            low += mag;
        } else if hz < 2000.0 {
            mid += mag;
        } else {
            high += mag;
        }
    }
    let share = |part: f64| if total > 0.0 { (part / total) as f32 } else { 0.0 };
    (
        if total > 0.0 {
            (weighted / total) as f32
        } else {
            0.0
        },
        share(low),
        share(mid),
        share(high),
    )
}

pub fn write_wav(path: &Path, sr: u32, left: &[f32], right: &[f32]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let n = left.len().min(right.len());
    let data_len = (n * 6) as u32;
    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes());
    buf.extend_from_slice(&sr.to_le_bytes());
    buf.extend_from_slice(&(sr * 6).to_le_bytes());
    buf.extend_from_slice(&6u16.to_le_bytes());
    buf.extend_from_slice(&24u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        for v in [left[i], right[i]] {
            let s = (v.clamp(-1.0, 1.0) * 8388607.0).round() as i32;
            let b = s.to_le_bytes();
            buf.extend_from_slice(&b[0..3]);
        }
    }
    fs::write(path, buf)
}

use nih_plug::prelude::util;

pub fn finish(left: &mut Vec<f32>, right: &mut Vec<f32>, sr: f32, peak_db: f32) -> SampleStats {
    trim_tail(left, right, sr);
    fade_edges(left, right, sr);
    normalize(left, right, peak_db);
    stats(left, right, sr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_readable_24_bit_stereo_wav() {
        let dir = std::env::temp_dir().join("lumen-packgen-test");
        let path = dir.join("tone.wav");
        let sr = 48000u32;
        let n = sr as usize / 10;
        let left: Vec<f32> = (0..n)
            .map(|i| (i as f32 / sr as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        let right = left.clone();
        write_wav(&path, sr, &left, &right).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 24);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        assert_eq!(bytes.len(), 44 + n * 6);
        let s = stats(&left, &right, sr as f32);
        assert!((s.centroid_hz - 440.0).abs() < 30.0, "centroid {}", s.centroid_hz);
        let _ = fs::remove_file(&path);
    }
}
