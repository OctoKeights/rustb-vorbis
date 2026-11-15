//! Times the decoder the way `compare/bench.c` times stb_vorbis: each file is opened and
//! decoded to the end `repeats` times, as interleaved 16-bit samples and frame by frame as
//! floats from memory, and as interleaved 16-bit samples read from disk as they are needed.
//!
//!   cargo run --release --example bench -- <repeats> <file.ogg>...

use rustb_vorbis::Vorbis;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let repeats: usize = args.next().and_then(|text| text.parse().ok()).unwrap_or(1);
    let (mut shorts, mut floats, mut disk, mut seconds) = (0f64, 0f64, 0f64, 0f64);
    let mut samples = 0u64;

    for path in args {
        let Ok(data) = std::fs::read(&path) else { continue };

        let start = Instant::now();
        for repeat in 0..repeats {
            let Ok(mut vorbis) = Vorbis::open(&data) else { break };
            let info = vorbis.info();
            let mut buffer = vec![0i16; 4096 * info.channels];
            loop {
                let n = vorbis.get_samples_short_interleaved(info.channels, &mut buffer);
                if n == 0 {
                    break;
                }
                samples += n as u64;
                if repeat == 0 {
                    seconds += n as f64 / f64::from(info.sample_rate);
                }
            }
        }
        shorts += start.elapsed().as_secs_f64();

        let start = Instant::now();
        for _ in 0..repeats {
            let Ok(mut vorbis) = Vorbis::open(&data) else { break };
            let mut empty = 0;
            while empty < 4 {
                let n = vorbis.get_frame_float();
                if n == 0 {
                    empty += 1;
                } else {
                    empty = 0;
                }
                samples += n as u64;
            }
        }
        floats += start.elapsed().as_secs_f64();

        let start = Instant::now();
        for _ in 0..repeats {
            let Ok(mut vorbis) = Vorbis::open_filename(&path) else { break };
            let info = vorbis.info();
            let mut buffer = vec![0i16; 4096 * info.channels];
            loop {
                let n = vorbis.get_samples_short_interleaved(info.channels, &mut buffer);
                if n == 0 {
                    break;
                }
                samples += n as u64;
            }
        }
        disk += start.elapsed().as_secs_f64();
    }
    println!("{seconds:.1} seconds of sound, {repeats} times each way ({samples} samples)");
    println!("16-bit samples: {shorts:.3} s ({:.0} times as fast as it plays)", seconds * repeats as f64 / shorts);
    println!("float frames:   {floats:.3} s ({:.0} times as fast as it plays)", seconds * repeats as f64 / floats);
    println!("16-bit, on disk: {disk:.3} s ({:.0} times as fast as it plays)", seconds * repeats as f64 / disk);
}
