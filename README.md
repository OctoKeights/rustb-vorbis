# rustb-vorbis

A pure-Rust Ogg Vorbis decoder, ported from Sean Barrett's
[stb_vorbis](https://github.com/nothings/stb/blob/master/stb_vorbis.c) (version 1.22).

The goal of this crate is simple: decode Ogg Vorbis exactly the way stb_vorbis does, in safe
Rust, with nothing else to install.

- **Bit-identical output.** The port follows stb_vorbis function by function and keeps its
  floating-point operations in the same order, so the samples you get are the same ones the C
  library produces, both as floats and as 16-bit integers.
- **No dependencies.** The crate uses only the Rust standard library.
- **No `unsafe` code.** The crate is built with `#![forbid(unsafe_code)]`, so a damaged or
  hostile file cannot make it read or write outside its buffers.

## Contents

- [Getting started](#getting-started)
- [Is this the right decoder for you?](#is-this-the-right-decoder-for-you)
- [API overview](#api-overview)
- [Matching a particular build of stb_vorbis](#matching-a-particular-build-of-stb_vorbis)
- [Behaviour on damaged files](#behaviour-on-damaged-files)
- [How the port is verified](#how-the-port-is-verified)
- [Limitations](#limitations)
- [Credits and licence](#credits-and-licence)

## Getting started

The crate is not on crates.io yet. Until it is, you can depend on this repository directly:

```toml
[dependencies]
rustb-vorbis = { git = "https://github.com/OctoKeights/rustb-vorbis" }
```

To decode a whole file in one call:

```rust
let file = std::fs::read("sound.ogg")?;
let (channels, sample_rate, samples) = rustb_vorbis::decode_memory(&file)?;
// `samples` holds interleaved 16-bit samples.
```

To decode a little at a time, which is what you want for streaming playback or long files:

```rust
use rustb_vorbis::Vorbis;

let file = std::fs::read("sound.ogg")?;
let mut vorbis = Vorbis::open(&file)?;
let info = vorbis.info();
println!(
    "{} channels at {} Hz, {} samples long",
    info.channels,
    info.sample_rate,
    vorbis.stream_length_in_samples()
);

let mut buffer = vec![0i16; 4096 * info.channels];
loop {
    let frames = vorbis.get_samples_short_interleaved(info.channels, &mut buffer);
    if frames == 0 {
        break; // end of the stream
    }
    // buffer[..frames * info.channels] is ready to play or write out
}
```

Seeking works on sample positions:

```rust
vorbis.seek(44_100)?; // the next sample read is sample 44,100
```

## Is this the right decoder for you?

Rust already has good Vorbis decoders written in Rust, notably
[lewton](https://crates.io/crates/lewton) and
[Symphonia](https://crates.io/crates/symphonia). If you would like a decoder that is part of a
wider audio library, or one that can read from a stream as it arrives, one of those is
probably the better choice.

rustb-vorbis is aimed at a narrower need:

- You are moving a C or C++ project that uses stb_vorbis to Rust, and its audio output must
  not change.
- You would like a small, self-contained decoder with no dependencies at all.

The crate was written for [Noolith](https://github.com/OctoKeights/Noolith), a project whose
C++ code decoded its sounds with stb_vorbis and whose Rust rewrite needed to produce exactly
the same audio.

## API overview

The crate covers stb_vorbis's "pull" API for a file held in memory. Function names follow the
originals with the `stb_vorbis_` prefix removed, so existing knowledge of the C library carries
over.

| stb_vorbis | rustb-vorbis |
| --- | --- |
| `stb_vorbis_open_memory` | `Vorbis::open` |
| `stb_vorbis_get_info`, `stb_vorbis_get_comment` | `info`, `vendor`, `comments` |
| `stb_vorbis_get_error` | `take_error` |
| `stb_vorbis_get_sample_offset`, `stb_vorbis_get_file_offset` | `sample_offset`, `file_offset` |
| `stb_vorbis_get_frame_float` | `get_frame_float`, then `frame(channel)` |
| `stb_vorbis_get_frame_short`, `..._short_interleaved` | `get_frame_short`, `get_frame_short_interleaved` |
| `stb_vorbis_get_samples_float`, `..._float_interleaved` | `get_samples_float`, `get_samples_float_interleaved` |
| `stb_vorbis_get_samples_short`, `..._short_interleaved` | `get_samples_short`, `get_samples_short_interleaved` |
| `stb_vorbis_seek`, `stb_vorbis_seek_frame`, `stb_vorbis_seek_start` | `seek`, `seek_frame`, `seek_start` |
| `stb_vorbis_stream_length_in_samples`, `..._in_seconds` | `stream_length_in_samples`, `stream_length_in_seconds` |
| `stb_vorbis_decode_memory` | `decode_memory` |

Errors are reported as a `VorbisError`, which carries the same numeric codes as stb_vorbis's
`STBVorbisError` (available through `VorbisError::code`).

## Matching a particular build of stb_vorbis

One detail is worth knowing if you need bit-exact output. stb_vorbis builds its lookup tables
from π, and where it gets π from depends on the compiler:

- Where `<math.h>` defines `M_PI` (gcc and clang on Linux, macOS and MinGW), it uses that
  `double`.
- Where it does not (Microsoft's compiler, unless `_USE_MATH_DEFINES` is set), it falls back
  to a `float` constant of its own.
- With that `float`, and the file compiled as C++, some of the cosines and sines are computed
  by the single-precision functions, because of C++ overloading.

The resulting audio is the same to the ear. The samples differ only in their last bits, and
occasionally a 16-bit sample differs by one. Since "the same as stb_vorbis" can therefore mean
three slightly different things, the crate lets you choose:

| You want to match | Use |
| --- | --- |
| stb_vorbis built by gcc or clang | `Vorbis::open` / `decode_memory` (the default, `Pi::Double`) |
| stb_vorbis built as C by Microsoft's compiler | `Vorbis::open_with_pi(data, Pi::Float)` |
| stb_vorbis built as C++ by Microsoft's compiler | `Vorbis::open_with_pi(data, Pi::FloatCpp)` |

`decode_memory_with_pi` takes the same choice. One note on `Pi::FloatCpp`: the
single-precision cosine and sine come from the system's maths library, so it matches a C++
build made on the same system it runs on.

If you only need correct audio and are not comparing against a C build, the default is the
one to use.

## Behaviour on damaged files

For valid Vorbis files, the port and stb_vorbis agree exactly. For damaged files, the port
follows stb_vorbis as far as stb_vorbis stays within its own memory, and in practice it almost
always returns the same samples, positions and error codes. The known differences are:

- **Out-of-bounds access.** Where stb_vorbis would read or write past a buffer, the port stops
  or reads zeros.
- **Assertions.** Where stb_vorbis asserts (for example, on out-of-order pages while seeking),
  the seek returns an error.
- **Overlong comments.** If a comment's length runs past the end of the comment header,
  stb_vorbis allocates the claimed length and pads it with bytes of 255. The port keeps only
  the bytes that are really there.
- **Overlong codebooks.** If a codebook claims more values than the setup header holds,
  stb_vorbis allocates the claimed size, keeps reading, and fails later. The port fails
  straight away, with the same error stb_vorbis would end on.
- **Failures without an error code.** stb_vorbis can occasionally fail to open a file without
  setting an error. The port reports `UnexpectedEof` in that case.
- **Not-a-number samples.** A file can contain values so extreme that the decoded samples are
  NaN. The sign of a NaN, and so whether it becomes -32768 or 32767 as a 16-bit sample,
  depends on the compiler. This is true of the C library as well.
- **Mono from multi-channel sound.** Asking `get_samples_short_interleaved` for one channel of
  a sound that has more is not supported by stb_vorbis (it asserts, or overruns the buffer if
  assertions are off). The port returns the same mono mix that `get_samples_short` gives.
- **Very large files.** Files larger than 2 GB are refused, because stb_vorbis takes the
  length as an `int`.

## How the port is verified

The repository includes a tool that checks the port against the C library directly.
`compare/dump.c` runs stb_vorbis over a set of files and prints the results of every function
in its pull API: each way of reading samples, down-mixing to one, two and three channels, ten
seeks to a sample and ten to a frame, and the decoder's position and error state after each
step. `examples/dump.rs` does the same with this crate, and `compare/compare.sh` builds both,
runs them over a folder of `.ogg` files and confirms that the output matches byte for byte:

```
compare/compare.sh <folder containing stb_vorbis.c> <folder of .ogg files> [double|float|float-cpp]
```

### Results so far

These comparisons were run on Linux (x86-64) against stb_vorbis built with gcc.

| Test set | Result |
| --- | --- |
| 23,622 sounds from three commercial games (S.T.A.L.K.E.R.: Shadow of Chernobyl, Clear Sky and Call of Pripyat; all mono) | Identical in the `double` and `float` builds: 968,502 lines of results each, with no differences. The same sounds also match in the `float-cpp` build, through Noolith's own comparison against its C++ code. |
| 140 synthetic sounds encoded with ffmpeg (1 to 8 channels, 8 to 96 kHz, every libvorbis quality from -1 to 10, fixed bitrates, ffmpeg's native Vorbis encoder, silence, a sound shorter than one frame, and sounds long enough to exercise seeking) | Identical in all three builds. |
| 3,600 randomly damaged copies of small sounds (changed bytes, truncated files, dropped or overwritten blocks) | 3,598 identical. stb_vorbis itself crashed on the other two. 2,213 of these files still open and decode. |
| Every single-bit change to the comment and setup headers of four small sounds (146,664 files, of which 64,267 still open) | 146,462 identical. stb_vorbis crashed or hung on 50. The remaining 152 are the differences described above: 105 in comment bytes, 43 in the sign of NaN samples, 3 where stb_vorbis fails without an error code, and one error code that the port has since been changed to match. |

The port did not panic on any of these files.

### What has not been verified

In the interest of being upfront about the gaps:

- For the `float` and `float-cpp` builds, gcc was given stb_vorbis's own constant in place of
  `M_PI`. The port has not yet been compared against a build made by Microsoft's compiler
  itself.
- No encoder available for testing writes residue type 0 or codebooks with accumulating values
  (`sequence_p`), so those paths are exercised only by damaged files, where a changed bit
  switches them on and both decoders agree on the resulting noise.
- No test file contains a codebook of lookup type 2, so that code has been checked only by
  reading it against the C source.
- The scripts that generated the damaged files are not part of this repository.

### Running the tests

`cargo test` needs no C compiler. It decodes the sounds in `tests/data` and compares the
results with what stb_vorbis printed for them, which is stored in `tests/data/expected-*.txt`.
The sounds are synthetic and were made for these tests; `tests/data/README.md` describes how.

## Limitations

- **Push API.** stb_vorbis's "push" API (`stb_vorbis_open_pushdata` and
  `stb_vorbis_decode_frame_pushdata`, for feeding the decoder a file in pieces) has not been
  ported. To decode from a file on disk, read it into memory and pass the bytes.
- **Inherited from stb_vorbis.** The port shares the original's limits: floor type 0 (found
  only in very old files, from before 2004) is not supported, only the first logical stream
  in a file is decoded, and page checksums are checked when seeking but not during normal
  decoding.
- **Speed.** The port currently takes about 25% longer than stb_vorbis built with `gcc -O2`.
  So far the priority has been matching the C library exactly; no performance work has been
  done yet.

## Credits and licence

All of the decoding logic is the work of Sean Barrett and the stb_vorbis contributors listed
in the [original source](https://github.com/nothings/stb/blob/master/stb_vorbis.c). This crate
is a translation of that work into Rust.

Like stb_vorbis, rustb-vorbis is available under either the MIT licence or the Unlicense
(public domain), whichever you prefer. See [`LICENSE`](LICENSE) for the full text.

Bug reports and contributions are welcome. If you find a file that this crate decodes
differently from stb_vorbis, please open an issue: that is exactly the kind of bug this
project exists to rule out.
