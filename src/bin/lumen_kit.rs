use lumen::packgen::{self, MixOptions, Mode};
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ! {
    eprintln!(
        "lumen-kit — render Lumen audio without a DAW

Usage:
  lumen-kit [--mode oneshots|groove|all] [--out DIR]
  lumen-kit --mixes N [--seed S] [--stems] [--out DIR]
  lumen-kit --score score.json [--stems] [--out DIR]
  lumen-kit --analyze DIR

  --mode   what to render (default: all)
  --mixes  render N different mixes (deterministic per seed)
  --score  render an external score JSON (notes + kit voices) into DIR
  --seed   first seed for --mixes (default: 1)
  --stems  also keep per-layer stems (default: mix only)
  --mp3    also write a 320 kbps mp3 next to every mix.wav (needs ffmpeg)
  --out    output directory (default: ~/Music Projects/Samples/lumen-kit)
  --analyze  print peak/rms/centroid/band share for every wav in DIR and exit"
    );
    std::process::exit(2)
}

fn main() -> ExitCode {
    let mut mode = Mode::All;
    let mut out = packgen::default_dir();
    let mut analyze: Option<PathBuf> = None;
    let mut mixes: Option<MixOptions> = None;
    let mut score: Option<PathBuf> = None;
    let mut stems = false;
    let mut mp3 = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                mode = args.get(i).map(|s| Mode::from(s.as_str())).unwrap_or(Mode::All);
            }
            "--mixes" => {
                i += 1;
                let count = args
                    .get(i)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(100);
                mixes.get_or_insert_with(MixOptions::default).count = count;
            }
            "--seed" => {
                i += 1;
                let seed = args
                    .get(i)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(1);
                mixes.get_or_insert_with(MixOptions::default).seed = seed;
            }
            "--stems" => {
                stems = true;
            }
            "--mp3" => {
                mp3 = true;
            }
            "--score" => {
                i += 1;
                score = args.get(i).map(PathBuf::from);
            }
            "--out" => {
                i += 1;
                match args.get(i) {
                    Some(p) => out = PathBuf::from(p),
                    None => usage(),
                }
            }
            "--analyze" => {
                i += 1;
                analyze = args.get(i).map(PathBuf::from);
            }
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
        i += 1;
    }

    if let Some(dir) = analyze {
        match packgen::analyze(&dir) {
            Ok(rows) => {
                println!(
                    "{:<26} {:>7} {:>7} {:>9} {:>7} {:>7} {:>7}",
                    "file", "peak", "rms", "centroid", "low", "mid", "high"
                );
                for row in rows {
                    let s = row.stats;
                    println!(
                        "{:<26} {:>7.1} {:>7.1} {:>9.0} {:>7.2} {:>7.2} {:>7.2}",
                        row.file, s.peak_db, s.rms_db, s.centroid_hz, s.band_low, s.band_mid, s.band_high
                    );
                }
            }
            Err(err) => {
                eprintln!("analyze failed: {err}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    let result = match (&mixes, &score) {
        (_, Some(path)) => match packgen::score::render_file(path, &out, stems, mp3) {
            Ok(report) => {
                println!(
                    "{} · {} bars · {:.1}s · {} layers · peak {:.1} dBFS -> {}",
                    report.title,
                    report.bars,
                    report.seconds,
                    report.layers.len(),
                    report.peak_db,
                    report.mix
                );
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("score render failed: {err}");
                return ExitCode::FAILURE;
            }
        },
        (Some(options), None) => {
            let options = MixOptions {
                keep_stems: stems || options.keep_stems,
                mp3: mp3 || options.mp3,
                ..*options
            };
            packgen::run_mixes(&out, &options)
        }
        (None, None) => packgen::run(mode, &out),
    };
    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("render failed: {err}");
            ExitCode::FAILURE
        }
    }
}
