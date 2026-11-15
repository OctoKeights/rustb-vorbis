//! Prints what the decoder makes of Ogg Vorbis files, through every function it has, in the
//! same form as `compare/dump.c` does with stb_vorbis: `compare/compare.sh` runs both and
//! compares them.
//!
//!   cargo run --release --example dump -- [--float-pi | --float-pi-cpp] <file.ogg>...
//!
//! `--float-pi`: as stb_vorbis built without the C library's M_PI; `--float-pi-cpp`: and as
//! C++ (see `Pi`).

use rustb_vorbis::{decode_memory_with_pi, Pi, Vorbis, VorbisError, VorbisPush};
use std::io::Write;

/// FNV-1a, 64 bits.
struct Hash(u64);

impl Hash {
    fn new() -> Hash {
        Hash(14695981039346656037)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(1099511628211);
        }
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn shorts(&mut self, samples: &[i16]) {
        for sample in samples {
            self.bytes(&sample.to_le_bytes());
        }
    }
    fn floats(&mut self, samples: &[f32]) {
        for sample in samples {
            self.u32(sample.to_bits());
        }
    }
}

fn error_code(vorbis: &mut Vorbis) -> i32 {
    vorbis.take_error().map_or(0, |error| error.code())
}

fn sample_offset(vorbis: &Vorbis) -> i32 {
    vorbis.sample_offset().map_or(-1, |offset| offset as i32)
}

/// Where the decoder is and the error it holds, which this clears.
fn state(vorbis: &mut Vorbis) -> String {
    format!("at {} {} error {}", sample_offset(vorbis), vorbis.file_offset(), error_code(vorbis))
}

/// Interleaved 16-bit samples for `channels` channels, `chunk` at a time, to the end.
fn short_interleaved(vorbis: &mut Vorbis, channels: usize, chunk: usize) -> String {
    let mut buffer = vec![0i16; chunk * channels];
    let mut hash = Hash::new();
    let mut total = 0usize;
    loop {
        let n = vorbis.get_samples_short_interleaved(channels, &mut buffer);
        if n == 0 {
            break;
        }
        hash.shorts(&buffer[..n * channels]);
        total += n;
    }
    format!("{total} {:016x}", hash.0)
}

/// 16-bit samples for `channels` channels, each in its own buffer.
fn short_planar(vorbis: &mut Vorbis, channels: usize, chunk: usize) -> String {
    let mut buffers = vec![vec![0i16; chunk]; channels];
    let mut hash = Hash::new();
    let mut total = 0usize;
    loop {
        let mut slices: Vec<&mut [i16]> = buffers.iter_mut().map(|buffer| buffer.as_mut_slice()).collect();
        let n = vorbis.get_samples_short(&mut slices);
        if n == 0 {
            break;
        }
        for buffer in &buffers {
            hash.shorts(&buffer[..n]);
        }
        total += n;
    }
    format!("{total} {:016x}", hash.0)
}

fn float_interleaved(vorbis: &mut Vorbis, channels: usize, chunk: usize) -> String {
    let mut buffer = vec![0f32; chunk * channels];
    let mut hash = Hash::new();
    let mut total = 0usize;
    loop {
        let n = vorbis.get_samples_float_interleaved(channels, &mut buffer);
        if n == 0 {
            break;
        }
        hash.floats(&buffer[..n * channels]);
        total += n;
    }
    format!("{total} {:016x}", hash.0)
}

fn float_planar(vorbis: &mut Vorbis, channels: usize, chunk: usize) -> String {
    let mut buffers = vec![vec![0f32; chunk]; channels];
    let mut hash = Hash::new();
    let mut total = 0usize;
    loop {
        let mut slices: Vec<&mut [f32]> = buffers.iter_mut().map(|buffer| buffer.as_mut_slice()).collect();
        let n = vorbis.get_samples_float(&mut slices);
        if n == 0 {
            break;
        }
        for buffer in &buffers {
            hash.floats(&buffer[..n]);
        }
        total += n;
    }
    format!("{total} {:016x}", hash.0)
}

/// Frame by frame as floats: how many frames, the samples in all, and a sum of each frame's
/// place, length and samples. A frame without samples does not end it; ten in a row do.
fn frames_float(vorbis: &mut Vorbis, channels: usize) -> String {
    let mut hash = Hash::new();
    let (mut frames, mut total, mut empty) = (0usize, 0usize, 0);
    while empty < 10 {
        let n = vorbis.get_frame_float();
        hash.u32(sample_offset(vorbis) as u32);
        hash.u32(n as u32);
        if n == 0 {
            empty += 1;
            continue;
        }
        empty = 0;
        for channel in 0..channels {
            hash.floats(vorbis.frame(channel));
        }
        frames += 1;
        total += n;
    }
    format!("{frames} {total} {:016x}", hash.0)
}

/// Frame by frame as interleaved 16-bit samples, with room for `room` samples.
fn frames_short_interleaved(vorbis: &mut Vorbis, channels: usize, room: usize) -> String {
    let mut buffer = vec![0i16; room];
    let mut hash = Hash::new();
    let (mut frames, mut total) = (0usize, 0usize);
    loop {
        let n = vorbis.get_frame_short_interleaved(channels, &mut buffer);
        if n == 0 {
            break;
        }
        hash.shorts(&buffer[..n * channels]);
        frames += 1;
        total += n;
    }
    format!("{frames} {total} {:016x}", hash.0)
}

/// Frame by frame as 16-bit samples, a buffer of `room` samples for each channel.
fn frames_short_planar(vorbis: &mut Vorbis, channels: usize, room: usize) -> String {
    let mut buffers = vec![vec![0i16; room]; channels];
    let mut hash = Hash::new();
    let (mut frames, mut total) = (0usize, 0usize);
    loop {
        let mut slices: Vec<&mut [i16]> = buffers.iter_mut().map(|buffer| buffer.as_mut_slice()).collect();
        let n = vorbis.get_frame_short(&mut slices);
        if n == 0 {
            break;
        }
        for buffer in &buffers {
            hash.shorts(&buffer[..n]);
        }
        frames += 1;
        total += n;
    }
    format!("{frames} {total} {:016x}", hash.0)
}

fn push_offset(vorbis: &VorbisPush) -> i32 {
    vorbis.sample_offset().map_or(-1, |offset| offset as i32)
}

/// The push API: the file handed over `chunk` bytes more at a time. Every call's results go
/// into one sum; then the decoder is told four times that it has been moved in the file
/// (flush) and has to find a page and decode a few frames from there.
fn push(data: &[u8], chunk: usize, pi: Pi, out: &mut Vec<u8>) {
    let size = data.len();
    let mut avail = chunk.min(size);
    let (mut vorbis, used) = loop {
        match VorbisPush::open_with_pi(&data[..avail], pi) {
            Ok(opened) => break opened,
            Err(VorbisError::NeedMoreData) if avail != size => avail = (avail + chunk).min(size),
            Err(error) => {
                let _ = writeln!(out, "push {chunk} open error {}", error.code());
                return;
            }
        }
    };
    let info = vorbis.info();
    let _ = writeln!(out, "push {chunk} open {used} {} {} {}", info.channels, info.sample_rate, info.max_frame_size);

    let mut hash = Hash::new();
    let mut pos = used;
    let (mut frames, mut total, mut steps) = (0usize, 0usize, 0usize);
    while steps < 50_000_000 {
        let step = vorbis.decode_frame(&data[pos..avail]);
        steps += 1;
        hash.u32(step.bytes_used as u32);
        hash.u32(step.samples as u32);
        hash.u32(vorbis.take_error().map_or(0, |error| error.code()) as u32);
        hash.u32(push_offset(&vorbis) as u32);
        if step.bytes_used == 0 {
            if avail == size {
                break;
            }
            avail = (avail + chunk).min(size);
            continue;
        }
        pos += step.bytes_used;
        if pos > avail {
            break;
        }
        if step.samples > 0 {
            for channel in 0..info.channels {
                hash.floats(&vorbis.frame(channel)[..step.samples]);
            }
            frames += 1;
            total += step.samples;
        }
    }
    let _ = writeln!(out, "push {chunk} frames {steps} {frames} {total} {:016x} at {pos} {}", hash.0, push_offset(&vorbis));

    for place in [0, size / 3, size / 2 + 17, size / 8 * 7] {
        let (mut found, mut good) = (-2i32, 0);
        let mut hash = Hash::new();
        let mut steps = 0usize;
        vorbis.flush();
        let mut pos = place.min(size);
        let mut avail = (pos + chunk).min(size);
        while good < 3 && steps < 50_000_000 {
            let step = vorbis.decode_frame(&data[pos..avail]);
            steps += 1;
            hash.u32(step.bytes_used as u32);
            hash.u32(step.samples as u32);
            hash.u32(vorbis.take_error().map_or(0, |error| error.code()) as u32);
            if found == -2 && push_offset(&vorbis) != -1 {
                found = push_offset(&vorbis);
            }
            if step.bytes_used == 0 {
                if avail == size {
                    break;
                }
                avail = (avail + chunk).min(size);
                continue;
            }
            pos += step.bytes_used;
            if pos > avail {
                break;
            }
            if step.samples > 0 {
                for channel in 0..info.channels {
                    hash.floats(&vorbis.frame(channel)[..step.samples]);
                }
                good += 1;
            }
        }
        let _ = writeln!(out, "push {chunk} flush {place} {steps} {good} {found} {:016x} at {pos} {}", hash.0, push_offset(&vorbis));
    }
}

/// Everything about one file, as text.
pub fn dump(path: &str, pi: Pi, out: &mut Vec<u8>) {
    let _ = writeln!(out, "file {}", path.rsplit(['/', '\\']).next().unwrap_or(path));
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(_) => {
            let _ = writeln!(out, "unreadable");
            return;
        }
    };
    let mut vorbis = match Vorbis::open_with_pi(&data, pi) {
        Ok(vorbis) => vorbis,
        Err(error) => {
            let _ = writeln!(out, "open error {}", error.code());
            return;
        }
    };
    let info = vorbis.info();
    let channels = info.channels;
    let _ = writeln!(out, "info {} {} {}", info.channels, info.sample_rate, info.max_frame_size);

    // (stb_vorbis gives the comments as C strings: up to their first zero byte.)
    let c_string = |text: &'_ [u8]| text.iter().position(|&byte| byte == 0).unwrap_or(text.len());
    let mut hash = Hash::new();
    let vendor = &vorbis.vendor()[..c_string(vorbis.vendor())];
    hash.bytes(vendor);
    for comment in vorbis.comments() {
        let comment = &comment[..c_string(comment)];
        hash.u32(comment.len() as u32);
        hash.bytes(comment);
    }
    let _ = writeln!(out, "comments {} {} {:016x}", vendor.len(), vorbis.comments().len(), hash.0);
    let _ = writeln!(out, "opened {}", state(&mut vorbis));

    let length = vorbis.stream_length_in_samples();
    let _ = writeln!(out, "length {} {:08x} {}", length, vorbis.stream_length_in_seconds().to_bits(), state(&mut vorbis));

    let _ = writeln!(out, "short {} {}", short_interleaved(&mut vorbis, channels, 4096), state(&mut vorbis));

    let _ = writeln!(out, "start {} {}", vorbis.seek_start().is_ok() as u8, state(&mut vorbis));
    let _ = writeln!(out, "frames {} {}", frames_float(&mut vorbis, channels), state(&mut vorbis));

    // (stb_vorbis can't interleave for one channel what has more: dump.c takes the mono mix of
    // its function for separate buffers here.)
    vorbis.seek_start().ok();
    let _ = writeln!(out, "mono {} {}", short_interleaved(&mut vorbis, 1, 1000), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "mono planar {} {}", short_planar(&mut vorbis, 1, 777), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "stereo {} {}", short_interleaved(&mut vorbis, 2, 1000), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "stereo planar {} {}", short_planar(&mut vorbis, 2, 3001), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "three {} {}", short_interleaved(&mut vorbis, 3, 500), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "three planar {} {}", short_planar(&mut vorbis, 3, 64), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "float one {} {}", float_interleaved(&mut vorbis, 1, 1024), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "float three {} {}", float_interleaved(&mut vorbis, 3, 333), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "float planar {} {}", float_planar(&mut vorbis, 2, 2048), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "frames short {} {}", frames_short_interleaved(&mut vorbis, channels, 4096 * channels), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "frames short cut {} {}", frames_short_interleaved(&mut vorbis, 2, 300), state(&mut vorbis));
    vorbis.seek_start().ok();
    let _ = writeln!(out, "frames planar {} {}", frames_short_planar(&mut vorbis, 2, 100), state(&mut vorbis));

    let targets = [0, 1, length / 3, length / 2, length.wrapping_sub(1), length, length.wrapping_add(1), length / 7 * 5, 12345 % length.wrapping_add(1).max(1), 0];
    for target in targets {
        let ok = vorbis.seek(target).is_ok() as u8;
        let at = state(&mut vorbis);
        let mut buffer = vec![0i16; 300 * channels];
        let n = vorbis.get_samples_short_interleaved(channels, &mut buffer);
        let mut hash = Hash::new();
        hash.shorts(&buffer[..n * channels]);
        let _ = writeln!(out, "seek {target} {ok} {at} {n} {:016x} {}", hash.0, state(&mut vorbis));

        let ok = vorbis.seek_frame(target).is_ok() as u8;
        let at = state(&mut vorbis);
        let n = vorbis.get_frame_float();
        let mut hash = Hash::new();
        for channel in 0..channels {
            hash.floats(vorbis.frame(channel));
        }
        let _ = writeln!(out, "seek frame {target} {ok} {at} {n} {:016x} {}", hash.0, state(&mut vorbis));
    }
    drop(vorbis);

    match decode_memory_with_pi(&data, pi) {
        Ok((channels, sample_rate, samples)) => {
            let mut hash = Hash::new();
            hash.shorts(&samples);
            let _ = writeln!(out, "whole {} {} {} {:016x}", channels, sample_rate, samples.len() / channels.max(1), hash.0);
        }
        Err(_) => {
            let _ = writeln!(out, "whole error");
        }
    }

    push(&data, data.len(), pi, out);
    push(&data, 4096, pi, out);
    push(&data, 517, pi, out);
    push(&data, 61, pi, out);
    // (A byte at a time, where that doesn't take all day.)
    if data.len() <= 20000 {
        push(&data, 1, pi, out);
    }
}

fn main() {
    let mut out = Vec::new();
    let mut pi = Pi::Double;
    for path in std::env::args().skip(1) {
        if path == "--float-pi" {
            pi = Pi::Float;
        } else if path == "--float-pi-cpp" {
            pi = Pi::FloatCpp;
        } else {
            dump(&path, pi, &mut out);
        }
    }
    let _ = std::io::stdout().write_all(&out);
}
