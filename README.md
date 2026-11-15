# rustb-vorbis

An Ogg Vorbis decoder in Rust: a port of Sean Barrett's
[stb_vorbis](https://github.com/nothings/stb/blob/master/stb_vorbis.c) (version 1.22),
function for function.

- **The same samples as the C, to the bit.** Every float operation is in stb_vorbis's order, so
  what comes out is what stb_vorbis gives, as floats and as 16-bit integers. This is checked
  against the C itself (see below).
- **No dependencies.**
- **No `unsafe`** (`#![forbid(unsafe_code)]`). A broken file can't make it read outside a
  buffer; it gets an error or silence, and none of the broken files tried has made it panic.

```rust
let file = std::fs::read("sound.ogg")?;
let mut vorbis = rustb_vorbis::Vorbis::open(&file)?;
let info = vorbis.info();
println!("{} channels at {} Hz, {} samples", info.channels, info.sample_rate, vorbis.stream_length_in_samples());

let mut samples = vec![0i16; 4096 * info.channels];
loop {
    let frames = vorbis.get_samples_short_interleaved(info.channels, &mut samples);
    if frames == 0 {
        break;
    }
    // samples[..frames * info.channels] is ready
}
```

Or all at once: `let (channels, sample_rate, samples) = rustb_vorbis::decode_memory(&file)?;`

## Is this the decoder you want?

Rust already has Vorbis decoders written in Rust:
[lewton](https://crates.io/crates/lewton) and
[symphonia](https://crates.io/crates/symphonia) (its `symphonia-codec-vorbis`). If you want a
maintained decoder that is part of a wider audio library, or one that reads from a stream as it
arrives, use one of those.

This one is for when you want what stb_vorbis does, exactly: you are moving a C or C++ program
that uses stb_vorbis to Rust and its output must not change, or you want one small crate with
nothing under it. It was written for [Noolith](https://github.com/OctoKeights/Noolith), whose
C++ decoded its sounds with stb_vorbis and whose Rust has to give the same sounds.

## What is here

stb_vorbis's "pull" API over a file held in memory:

| stb_vorbis | here |
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

Not here: the "push" API (`stb_vorbis_open_pushdata`, `stb_vorbis_decode_frame_pushdata`: feeding
the decoder a file in pieces), and reading from a `FILE*`: read the file and pass its bytes.

What stb_vorbis doesn't do, this doesn't either: floor 0 (found in old files, from before
2004), more than the first logical stream of a file, and checking page checksums while decoding
(it checks them when it seeks).

## Which stb_vorbis

stb_vorbis makes its tables with the C library's `M_PI`, a `double`. Where `<math.h>` doesn't
define it — Microsoft's compiler, unless `_USE_MATH_DEFINES` is set — it uses a constant of its
own, which is a `float`; and then, if the file is compiled as C++, the cosines and sines of some
tables are taken with the float functions, by C++'s overloading. The samples of these builds
differ in their last bits, and now and then a 16-bit sample by one. So "the same as stb_vorbis"
is three things, and all are here:

- `Vorbis::open`, `decode_memory`: `Pi::Double`, stb_vorbis as gcc and clang build it.
- `Vorbis::open_with_pi(data, Pi::Float)`, `decode_memory_with_pi`: as Microsoft's compiler
  builds it as C.
- `Pi::FloatCpp`: as Microsoft's compiler builds it as C++. The float cosine and sine are the
  system's own, so this one is the same as a build on the system it runs on.

## Where it differs from stb_vorbis

On files that are valid Vorbis it doesn't. On broken ones it follows stb_vorbis as far as
stb_vorbis stays inside its own memory, and nearly always gives the same samples, places and
error codes (see the next section). Where it can't or shouldn't:

- Where stb_vorbis would read or write past a buffer, this stops or reads zeros.
- Where stb_vorbis asserts (in seeking, on pages that are out of order), the seek fails.
- A comment whose length runs past the end of the comment header: stb_vorbis allocates that
  length and fills what the header doesn't have with bytes of 255. This keeps the bytes the
  header has.
- A codebook that claims more values than the setup header holds: stb_vorbis allocates and
  reads on, and then fails. This fails at once, with the error stb_vorbis ends at.
- stb_vorbis can fail to open a file without setting an error (an empty packet where the
  comment header should be). This says `UnexpectedEof`.
- A file can hold values so large that the samples are not numbers (NaN). Their signs, and so
  whether a 16-bit sample is -32768 or 32767, depend on the compiler, in C as well.
- `get_samples_short_interleaved` asked for one channel of a sound that has more: stb_vorbis
  asserts, or without asserts writes two channels into the buffer. This gives the mono mix
  that `get_samples_short` gives.
- A file larger than 2 GB is refused (stb_vorbis takes the length as an `int`).

## Checked against the C

`compare/compare.sh` builds `compare/dump.c` (stb_vorbis, calling every function of its pull
API on each file: every way of reading, down-mixing to one, two and three channels, ten seeks
to a sample and ten to a frame, and what the decoder says about where it is and what went
wrong after each) and `examples/dump.rs` (the same with this crate), runs both over a folder of
`.ogg` files and compares what they print:

    compare/compare.sh <folder holding stb_vorbis.c> <folder of .ogg files> [double|float|float-cpp]

What has been compared so far, on Linux (x86-64) against stb_vorbis built by gcc. For `float`
and `float-cpp`, gcc was given stb_vorbis's own constant in place of `M_PI`; nothing has been
compared with a build by Microsoft's compiler itself yet.

- **23,622 sounds of three games** (S.T.A.L.K.E.R.: Shadow of Chernobyl, Clear Sky and Call of
  Pripyat; all mono, by the libvorbis of their time), as `double` and as `float`: no
  difference, in 968,502 lines of results each. The same sounds decode to the same samples as
  `float-cpp` too, through Noolith's own check against its C++.
- **140 made-up sounds**, encoded for this with ffmpeg: one to eight channels, 8 to 96 kHz,
  libvorbis at every quality from -1 to 10 and at fixed bitrates, ffmpeg's own Vorbis encoder,
  silence, a sound shorter than a frame, sounds long enough for a seek to have to search. All
  three builds: no difference.
- **3,600 broken copies** of small sounds (bytes changed in the headers and in the audio, files
  cut short, blocks dropped or overwritten): 3,598 the same; on the other two stb_vorbis
  itself crashed. 2,213 of them still open and decode.
- **Every single-bit change** to the comment and setup headers of four small sounds (146,664
  files, 64,267 of which still open): 146,462 the same; stb_vorbis crashed or hung on 50; the
  other 152 are the differences listed above (105 in a comment's bytes, 43 in the signs of
  NaN samples, 3 where stb_vorbis fails without an error, and one error code that the port
  has since been made to follow).

The port did not panic on any of them. What these don't reach: no encoder here writes residue
type 0 or codebooks whose values accumulate (`sequence_p`), so those are only in broken copies
(a changed bit turns them on, and both decoders then agree on the noise); and a codebook of
lookup type 2 is in no file here at all, so that part is checked only by reading it against
the C. The scripts that made the broken copies are not in this repository.

`cargo test` checks the sounds in `tests/data` against what the C printed for them
(`tests/data/expected-*.txt`), so it needs no C compiler.

It is slower than the C: about a quarter more time than stb_vorbis built with `gcc -O2`, on
the comparison above. Nothing has been done for speed yet beyond following the C.

## Licence

As stb_vorbis: MIT or public domain (the Unlicense), whichever you prefer. See `LICENSE`.
stb_vorbis is by Sean Barrett, with the contributors its source lists.
