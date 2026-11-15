//! How the decoder behaves, on the sounds in `tests/data` and on broken copies of them.

use rustb_vorbis::{decode_memory, decode_memory_with_pi, Pi, Vorbis, VorbisError};

fn sound(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Decodes everything through every channel's own count, and gives the samples.
fn all_samples(data: &[u8]) -> (usize, Vec<i16>) {
    let mut vorbis = Vorbis::open(data).unwrap();
    let channels = vorbis.info().channels;
    let mut samples = Vec::new();
    let mut buffer = vec![0i16; 1000 * channels];
    loop {
        let n = vorbis.get_samples_short_interleaved(channels, &mut buffer);
        if n == 0 {
            break;
        }
        samples.extend_from_slice(&buffer[..n * channels]);
    }
    (channels, samples)
}

#[test]
fn what_the_sounds_are() {
    let data = sound("stereo_48k_q5.ogg");
    let mut vorbis = Vorbis::open(&data).unwrap();
    let info = vorbis.info();
    assert_eq!((info.channels, info.sample_rate, info.max_frame_size), (2, 48000, 1024));
    assert_eq!(vorbis.stream_length_in_samples(), 48000);
    assert_eq!(vorbis.stream_length_in_seconds(), 1.0);
    assert_eq!(vorbis.sample_offset(), Some(0));
    assert_eq!(vorbis.take_error(), None);
    assert!(vorbis.vendor().starts_with(b"Lavf"));
    assert!(vorbis.comments().iter().any(|comment| comment.starts_with(b"encoder=")));

    for (name, channels, sample_rate, length) in [("mono_44k_q2.ogg", 1, 44100, 44100), ("six_48k_q3.ogg", 6, 48000, 24000), ("eight_44k_q4.ogg", 8, 44100, 17640), ("mono_tiny.ogg", 1, 44100, 441)] {
        let data = sound(name);
        let (decoded_channels, decoded_rate, samples) = decode_memory(&data).unwrap();
        assert_eq!((decoded_channels, decoded_rate), (channels, sample_rate), "{name}");
        assert_eq!(samples.len(), length * channels, "{name}");
        // The chirp is there: not silence.
        let loudest = samples.iter().map(|&sample| i32::from(sample).abs()).max().unwrap();
        assert!(loudest > 2000, "{name}: {loudest}");
        assert_eq!(all_samples(&data), (channels, samples), "{name}");
    }
}

#[test]
fn the_ways_of_reading_agree() {
    let data = sound("stereo_48k_q5.ogg");
    let (channels, whole) = all_samples(&data);
    assert_eq!(channels, 2);

    // Separate buffers.
    let mut vorbis = Vorbis::open(&data).unwrap();
    let (mut left, mut right) = (vec![0i16; 48000 + 10], vec![0i16; 48000 + 10]);
    assert_eq!(vorbis.get_samples_short(&mut [&mut left, &mut right]), 48000);
    for i in 0..48000 {
        assert_eq!((left[i], right[i]), (whole[i * 2], whole[i * 2 + 1]));
    }
    // (The end of the stream: nothing more, and no error.)
    assert_eq!(vorbis.get_samples_short(&mut [&mut left, &mut right]), 0);
    assert_eq!(vorbis.take_error(), None);

    // Floats, frame by frame: the same samples before they were rounded.
    vorbis.seek_start().unwrap();
    let mut at = 0usize;
    loop {
        let n = vorbis.get_frame_float();
        if n == 0 {
            break;
        }
        assert_eq!(vorbis.frame(0).len(), n);
        for i in 0..n {
            for channel in 0..2 {
                let exact = f64::from(vorbis.frame(channel)[i]) * 32768.0;
                let sample = f64::from(whole[(at + i) * 2 + channel]);
                assert!((exact.clamp(-32768.0, 32767.0) - sample).abs() <= 1.0, "sample {} of channel {channel}: {exact} and {sample}", at + i);
            }
        }
        at += n;
    }
    assert_eq!(at, 48000);
    assert_eq!(vorbis.frame(0).len(), 0);
    assert_eq!(vorbis.frame(99).len(), 0);

    // More channels than the stream has are silent; fewer leaves the rest out.
    vorbis.seek_start().unwrap();
    let mut floats = vec![1.0f32; 3 * 100];
    assert_eq!(vorbis.get_samples_float_interleaved(3, &mut floats), 100);
    assert!(floats.chunks(3).all(|frame| frame[2] == 0.0));
    assert!(floats.chunks(3).any(|frame| frame[0] != 0.0 && frame[1] != 0.0));

    // Nothing asked for, nothing given.
    assert_eq!(vorbis.get_samples_short_interleaved(0, &mut [0i16; 8]), 0);
    assert_eq!(vorbis.get_samples_short_interleaved(2, &mut []), 0);
    assert_eq!(vorbis.get_samples_short(&mut []), 0);
    assert_eq!(vorbis.get_samples_float(&mut []), 0);
}

#[test]
fn mixing_down() {
    // One channel from two is their sum (stb_vorbis doesn't halve it).
    let data = sound("stereo_48k_q5.ogg");
    let mut vorbis = Vorbis::open(&data).unwrap();
    let mut both = vec![0f32; 2 * 4800];
    assert_eq!(vorbis.get_samples_float_interleaved(2, &mut both), 4800);
    vorbis.seek_start().unwrap();
    let mut mono = vec![0i16; 4800];
    assert_eq!(vorbis.get_samples_short_interleaved(1, &mut mono), 4800);
    for i in 0..4800 {
        let sum = (f64::from(both[i * 2]) + f64::from(both[i * 2 + 1])) * 32768.0;
        assert!((sum.clamp(-32768.0, 32767.0) - f64::from(mono[i])).abs() <= 1.0, "sample {i}");
    }

    // Two channels from 5.1: something on both sides.
    let data = sound("six_48k_q3.ogg");
    let mut vorbis = Vorbis::open(&data).unwrap();
    let mut stereo = vec![0i16; 2 * 24000];
    assert_eq!(vorbis.get_samples_short_interleaved(2, &mut stereo), 24000);
    assert!(stereo.chunks(2).any(|frame| frame[0] != 0) && stereo.chunks(2).any(|frame| frame[1] != 0));
    assert!(stereo.chunks(2).any(|frame| frame[0] != frame[1]));
}

#[test]
fn the_builds_of_stb_vorbis_differ_by_a_sample_at_most() {
    for name in ["mono_44k_q2.ogg", "stereo_48k_q5.ogg", "six_48k_q3.ogg", "stereo_native.ogg"] {
        let data = sound(name);
        let (_, _, double) = decode_memory_with_pi(&data, Pi::Double).unwrap();
        assert_eq!(decode_memory(&data).unwrap().2, double);
        let mut different = 0;
        for pi in [Pi::Float, Pi::FloatCpp] {
            let (_, _, other) = decode_memory_with_pi(&data, pi).unwrap();
            assert_eq!(other.len(), double.len());
            for (a, b) in double.iter().zip(&other) {
                assert!((i32::from(*a) - i32::from(*b)).abs() <= 1, "{name}: {a} and {b}");
                different += usize::from(a != b);
            }
        }
        // (They are different builds: if nothing differed, the choice would not be reaching
        // the tables.)
        assert!(different > 0, "{name}");
    }
}

#[test]
fn seeking() {
    let data = sound("mono_long.ogg");
    let (_, whole) = all_samples(&data);
    let mut vorbis = Vorbis::open(&data).unwrap();
    let length = vorbis.stream_length_in_samples();
    assert_eq!(length as usize, whole.len());
    assert_eq!(length, 30 * 44100);

    // Anywhere in the stream: the samples from there on are the ones a decode from the start
    // gives.
    let mut buffer = vec![0i16; 3000];
    let mut target = 0u32;
    for step in 0..60u32 {
        vorbis.seek(target).unwrap();
        assert_eq!(vorbis.take_error(), None);
        let n = vorbis.get_samples_short_interleaved(1, &mut buffer);
        let from = target as usize;
        assert_eq!(n, (whole.len() - from).min(3000), "at {target}");
        assert_eq!(&buffer[..n], &whole[from..from + n], "at {target}");
        // (Forwards and backwards, near and far.)
        target = (target.wrapping_mul(2_654_435_761).wrapping_add(step * 104_729)) % (length + 1);
    }

    // To a frame: the frame holds the sample.
    vorbis.seek_frame(700_000).unwrap();
    let start = vorbis.sample_offset().unwrap();
    let n = vorbis.get_frame_float() as u32;
    assert!(start <= 700_000 && 700_000 < start + n, "{start} + {n}");

    // The end is a place; past it is not.
    vorbis.seek(length).unwrap();
    assert_eq!(vorbis.get_samples_short_interleaved(1, &mut buffer), 0);
    assert_eq!(vorbis.seek(length + 1), Err(VorbisError::SeekInvalid));
    assert_eq!(vorbis.take_error(), Some(VorbisError::SeekInvalid));
    assert_eq!(vorbis.take_error(), None);

    // And back to the start.
    vorbis.seek_start().unwrap();
    assert_eq!(vorbis.sample_offset(), Some(0));
    let n = vorbis.get_samples_short_interleaved(1, &mut buffer);
    assert_eq!(&buffer[..n], &whole[..3000]);
}

#[test]
fn what_is_not_a_sound() {
    assert!(Vorbis::open(&[]).is_err());
    assert_eq!(Vorbis::open(b"RIFF....WAVEfmt ").err(), Some(VorbisError::MissingCapturePattern));
    assert_eq!(Vorbis::open(&[0x4f, 0x67, 0x67, 0x53]).err(), Some(VorbisError::InvalidFirstPage));
    assert!(decode_memory(&[0u8; 4096]).is_err());
    assert_eq!(VorbisError::InvalidSetup.code(), 20);
    assert_eq!(VorbisError::SeekFailed.to_string(), "the seek failed");
}

/// xorshift: the same broken files every time.
fn next(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

/// Everything the decoder can be asked, on a file that may be anything.
fn exercise(data: &[u8]) {
    let Ok(mut vorbis) = Vorbis::open(data) else { return };
    let info = vorbis.info();
    assert!((1..=rustb_vorbis::MAX_CHANNELS).contains(&info.channels));
    let length = vorbis.stream_length_in_samples();
    let mut shorts = vec![0i16; 2 * 1024];
    let mut floats = vec![0f32; 3 * 500];
    let mut frames = 0;
    while vorbis.get_samples_short_interleaved(2, &mut shorts) != 0 && frames < 4000 {
        frames += 1;
    }
    for target in [0, length / 2, length, length.wrapping_add(7)] {
        let _ = vorbis.seek(target);
        vorbis.get_samples_float_interleaved(3, &mut floats);
        let _ = vorbis.seek_frame(target);
        vorbis.get_frame_float();
        for channel in 0..info.channels {
            assert!(vorbis.frame(channel).len() <= info.max_frame_size * 2);
        }
        vorbis.get_frame_short_interleaved(1, &mut shorts);
    }
    let _ = vorbis.seek_start();
    vorbis.take_error();
}

#[test]
fn broken_files_are_errors_not_panics() {
    for name in ["mono_44k_q2.ogg", "stereo_native.ogg", "six_48k_q3.ogg", "mono_tiny.ogg"] {
        let data = sound(name);
        // Cut short anywhere.
        for cut in (0..data.len()).step_by(61) {
            exercise(&data[..cut]);
        }
        // Bytes changed: in the headers, in the audio, and anywhere.
        let mut state = 0x2545_f491u32 ^ data.len() as u32;
        for round in 0..300 {
            let mut broken = data.clone();
            let within = match round % 3 {
                0 => broken.len().min(200),
                1 => broken.len().min(4000),
                _ => broken.len(),
            };
            for _ in 0..1 + next(&mut state) % 4 {
                let at = next(&mut state) as usize % within;
                broken[at] = if round % 2 == 0 { next(&mut state) as u8 } else { broken[at] ^ (1 << (next(&mut state) % 8)) };
            }
            exercise(&broken);
        }
    }
}
