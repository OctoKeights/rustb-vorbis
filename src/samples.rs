//! Turning the decoder's float channels into 16-bit samples, with stb_vorbis's down-mix to
//! one or two channels.

const PLAYBACK_MONO: i8 = 1;
const PLAYBACK_LEFT: i8 = 2;
const PLAYBACK_RIGHT: i8 = 4;

const L: i8 = PLAYBACK_LEFT | PLAYBACK_MONO;
const C: i8 = PLAYBACK_LEFT | PLAYBACK_RIGHT | PLAYBACK_MONO;
const R: i8 = PLAYBACK_RIGHT | PLAYBACK_MONO;

/// Where each channel of a 1 to 6 channel stream plays.
const CHANNEL_POSITION: [[i8; 6]; 7] = [[0, 0, 0, 0, 0, 0], [C, 0, 0, 0, 0, 0], [L, R, 0, 0, 0, 0], [L, C, R, 0, 0, 0], [L, R, L, R, 0, 0], [L, C, R, L, R, 0], [L, C, R, L, R, C]];

const BUFFER_SIZE: usize = 32;

/// stb_vorbis's FAST_SCALED_FLOAT_TO_INT with 15 bits: a float as a 16-bit sample, rounded by
/// adding a magic number and reading the float's bits.
#[inline]
pub(crate) fn float_to_i16(x: f32) -> i16 {
    const MAGIC: f32 = 1.5f32 * (1 << (23 - 15)) as f32 + 0.5f32 / (1 << 15) as f32;
    const ADDEND: i32 = ((150 - 15) << 23) + (1 << 22);
    let mut v = ((x + MAGIC).to_bits() as i32).wrapping_sub(ADDEND);
    if v.wrapping_add(32768) as u32 > 65535 {
        v = if v < 0 { -32768 } else { 32767 };
    }
    v as i16
}

fn copy_samples(dest: &mut [i16], src: &[f32]) {
    for (out, &sample) in dest.iter_mut().zip(src) {
        *out = float_to_i16(sample);
    }
}

/// One output channel as the sum of the stream's channels that play there.
fn compute_samples(mask: i8, output: &mut [i16], num_c: usize, data: &[Vec<f32>], d_offset: usize, len: usize) {
    let mut buffer = [0.0f32; BUFFER_SIZE];
    let mut n = BUFFER_SIZE;
    let mut o = 0usize;
    while o < len {
        buffer.fill(0.0);
        if o + n > len {
            n = len - o;
        }
        for j in 0..num_c {
            if CHANNEL_POSITION[num_c][j] & mask != 0 {
                for i in 0..n {
                    buffer[i] += data[j][d_offset + o + i];
                }
            }
        }
        for i in 0..n {
            output[o + i] = float_to_i16(buffer[i]);
        }
        o += BUFFER_SIZE;
    }
}

/// Stereo, interleaved, from a stream of up to six channels.
fn compute_stereo_samples(output: &mut [i16], num_c: usize, data: &[Vec<f32>], d_offset: usize, len: usize) {
    let mut buffer = [0.0f32; BUFFER_SIZE];
    let mut n = BUFFER_SIZE >> 1;
    // o is the offset in the source data
    let mut o = 0usize;
    while o < len {
        // o2 is the offset in the output data
        let o2 = o << 1;
        buffer.fill(0.0);
        if o + n > len {
            n = len - o;
        }
        for j in 0..num_c {
            let m = CHANNEL_POSITION[num_c][j] & (PLAYBACK_LEFT | PLAYBACK_RIGHT);
            if m == (PLAYBACK_LEFT | PLAYBACK_RIGHT) {
                for i in 0..n {
                    buffer[i * 2] += data[j][d_offset + o + i];
                    buffer[i * 2 + 1] += data[j][d_offset + o + i];
                }
            } else if m == PLAYBACK_LEFT {
                for i in 0..n {
                    buffer[i * 2] += data[j][d_offset + o + i];
                }
            } else if m == PLAYBACK_RIGHT {
                for i in 0..n {
                    buffer[i * 2 + 1] += data[j][d_offset + o + i];
                }
            }
        }
        for i in 0..(n << 1) {
            output[o2 + i] = float_to_i16(buffer[i]);
        }
        o += BUFFER_SIZE >> 1;
    }
}

/// `samples` samples of each of `buffers`, from `b_offset` on, out of `data` from `d_offset`.
pub(crate) fn convert_samples_short(buffers: &mut [&mut [i16]], b_offset: usize, data_c: usize, data: &[Vec<f32>], d_offset: usize, samples: usize) {
    let buf_c = buffers.len();
    if buf_c != data_c && buf_c <= 2 && data_c <= 6 {
        const CHANNEL_SELECTOR: [[i8; 2]; 3] = [[0, 0], [PLAYBACK_MONO, 0], [PLAYBACK_LEFT, PLAYBACK_RIGHT]];
        for i in 0..buf_c {
            compute_samples(CHANNEL_SELECTOR[buf_c][i], &mut buffers[i][b_offset..], data_c, data, d_offset, samples);
        }
    } else {
        let limit = buf_c.min(data_c);
        for i in 0..limit {
            copy_samples(&mut buffers[i][b_offset..b_offset + samples], &data[i][d_offset..d_offset + samples]);
        }
        for buffer in buffers.iter_mut().skip(limit) {
            buffer[b_offset..b_offset + samples].fill(0);
        }
    }
}

/// `len` samples for `buf_c` interleaved channels into `buffer`.
pub(crate) fn convert_channels_short_interleaved(buf_c: usize, buffer: &mut [i16], data_c: usize, data: &[Vec<f32>], d_offset: usize, len: usize) {
    if buf_c == 1 && data_c != 1 && data_c <= 6 {
        // (stb_vorbis asserts that this is stereo, and without the assert writes two channels
        // into a buffer for one. One channel is mixed the way its other functions do it.)
        compute_samples(PLAYBACK_MONO, buffer, data_c, data, d_offset, len);
    } else if buf_c != data_c && buf_c <= 2 && data_c <= 6 {
        // (stb_vorbis does this once for each of the two channels.)
        for _ in 0..buf_c {
            compute_stereo_samples(buffer, data_c, data, d_offset, len);
        }
    } else {
        let limit = buf_c.min(data_c);
        let mut at = 0usize;
        for j in 0..len {
            for channel in data.iter().take(limit) {
                buffer[at] = float_to_i16(channel[d_offset + j]);
                at += 1;
            }
            for _ in limit..buf_c {
                buffer[at] = 0;
                at += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_to_samples() {
        assert_eq!(float_to_i16(0.0), 0);
        assert_eq!(float_to_i16(0.5), 16384);
        assert_eq!(float_to_i16(-0.5), -16384);
        assert_eq!(float_to_i16(1.0), 32767); // clamped
        assert_eq!(float_to_i16(-1.0), -32768);
        assert_eq!(float_to_i16(7.5), 32767);
        assert_eq!(float_to_i16(-7.5), -32768);
        assert_eq!(float_to_i16(1.0 / 32768.0), 1);
    }

    #[test]
    fn down_mixing() {
        // Three channels (left, centre, right) to stereo: the centre goes to both sides.
        let data = vec![vec![0.25f32; 40], vec![0.125; 40], vec![-0.25; 40]];
        let mut stereo = [0i16; 80];
        convert_channels_short_interleaved(2, &mut stereo, 3, &data, 0, 40);
        assert!(stereo.chunks(2).all(|pair| pair == [12288, -4096]));
        // The same to mono: all three.
        let mut mono = [0i16; 40];
        convert_samples_short(&mut [&mut mono[..]], 0, 3, &data, 0, 40);
        assert!(mono.iter().all(|&sample| sample == 4096));
        // More output channels than the stream has: the extra ones are silent.
        let mut four = [1i16; 16];
        convert_channels_short_interleaved(4, &mut four, 3, &data, 0, 4);
        assert_eq!(four[..4], [8192, 4096, -8192, 0]);
    }
}
