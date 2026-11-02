use std::io;
use std::path::Path;
use std::process::Command;

pub const DEFAULT_BITRATE_KBPS: u32 = 320;

pub fn available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn encode(wav: &Path, mp3: &Path, kbps: u32) -> io::Result<()> {
    let missing = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            "ffmpeg not found in PATH; install it with `brew install ffmpeg` or drop --mp3",
        )
    };
    if !available() {
        return Err(missing());
    }
    let bitrate = format!("{}k", kbps.clamp(96, 320));
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(wav)
        .args(["-codec:a", "libmp3lame", "-b:a", &bitrate])
        .arg(mp3)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "ffmpeg failed for {}: {}",
                wav.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    if !mp3.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("ffmpeg reported success but {} is missing", mp3.display()),
        ));
    }
    Ok(())
}

pub fn alongside(wav: &Path, kbps: u32) -> io::Result<()> {
    let mp3 = wav.with_extension("mp3");
    encode(wav, &mp3, kbps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_short_wav_when_ffmpeg_is_present() {
        if !available() {
            return;
        }
        let dir = std::env::temp_dir().join("lumen-mp3-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sr = crate::packgen::sample_rate();
        let n = (sr * 0.25) as usize;
        let left: Vec<f32> = (0..n)
            .map(|i| (i as f32 / sr * 440.0 * std::f32::consts::TAU).sin() * 0.4)
            .collect();
        let right = left.clone();
        let wav = dir.join("tone.wav");
        crate::packgen::wav::write_wav(&wav, sr as u32, &left, &right).unwrap();
        alongside(&wav, 192).unwrap();
        let mp3 = wav.with_extension("mp3");
        let meta = std::fs::metadata(&mp3).unwrap();
        assert!(meta.len() > 1000, "mp3 is {} bytes", meta.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
