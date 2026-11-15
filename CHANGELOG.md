# Changelog

## 0.1.1

- Added `VorbisPush`, the port of stb_vorbis's push API, for decoding a file that arrives in
  pieces: `VorbisPush::open`, `decode_frame`, `frame` and `flush`.
- The comparison against the C library (`compare/compare.sh`) and the tests now cover the push
  API as well.
- No changes to the output or behaviour of the existing API.

## 0.1.0

- First release: the port of stb_vorbis's pull API for a file held in memory (`Vorbis`,
  `decode_memory`), with the three builds of stb_vorbis selectable through `Pi`.
