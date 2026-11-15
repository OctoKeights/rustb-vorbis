# Changelog

## 0.1.2

- Added reading from a file on disk, which completes the port of stb_vorbis's API:
  `Vorbis::open_filename`, `Vorbis::open_file`, `Vorbis::open_file_section` and
  `decode_filename`. The file is read a few kilobytes at a time as it is decoded.
- Decoding is about twice as fast as in 0.1.1, and now faster than stb_vorbis itself built by
  gcc or clang (the README has the measurements). Among the changes: the inverse MDCT and
  the other per-sample loops no longer check every index, 16-bit samples are converted
  several at a time, the residue has a tight loop for its usual case, and decoding a packet
  allocates nothing.
- The output is unchanged: every sample is still the one stb_vorbis produces.
- The comparison against the C library and the tests now cover the file functions, and
  `compare/bench.c` and `examples/bench.rs` time the two decoders the same way.

## 0.1.1

- Added `VorbisPush`, the port of stb_vorbis's push API, for decoding a file that arrives in
  pieces: `VorbisPush::open`, `decode_frame`, `frame` and `flush`.
- The comparison against the C library (`compare/compare.sh`) and the tests now cover the push
  API as well.
- No changes to the output or behaviour of the existing API.

## 0.1.0

- First release: the port of stb_vorbis's pull API for a file held in memory (`Vorbis`,
  `decode_memory`), with the three builds of stb_vorbis selectable through `Pi`.
