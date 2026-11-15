# Test sounds

The `.ogg` files here are made-up sounds (chirps and noise), encoded with ffmpeg for these
tests: one to eight channels, libvorbis and ffmpeg's own Vorbis encoder, a sound shorter than
a frame, and one long enough (`mono_long.ogg`) for a seek to have to search.
`mono_empty_page.ogg` has many small pages and, before the last of them, a page with nothing
in it. stb_vorbis reads such a page from memory, but from a file on disk it takes it for the
end of the file (C's `fread` of no bytes counts as a failure), and the port has to do both.

`expected-double.txt` and `expected-float.txt` are what stb_vorbis itself makes of them, as
`compare/dump.c` prints it, built for each of its two values of π (see `Pi`). `tests/golden.rs`
checks that the port prints the same. To make them again:

    compare/compare.sh <folder holding stb_vorbis.c> tests/data double
    cp "${TMPDIR:-/tmp}/rustb-vorbis-compare.c.txt" tests/data/expected-double.txt
    compare/compare.sh <folder holding stb_vorbis.c> tests/data float
    cp "${TMPDIR:-/tmp}/rustb-vorbis-compare.c.txt" tests/data/expected-float.txt

How the sounds were made (`chirp N` stands for N channels of
`0.4*sin(2*PI*(110*c+60*c*t)*t)*exp(-0.3*t)+0.05*sin(2*PI*(1000+317*c)*t)`, c from 1):

    ffmpeg -f lavfi -i "aevalsrc=exprs='<chirp 1>':s=44100:d=1.0"  -c:a libvorbis -q:a 2 -ac 1 mono_44k_q2.ogg
    ffmpeg -f lavfi -i "aevalsrc=exprs='<chirp 2>':s=48000:d=1.0"  -c:a libvorbis -q:a 5 stereo_48k_q5.ogg
    ffmpeg -f lavfi -i "aevalsrc=exprs='<chirp 6>':s=48000:d=0.5"  -channel_layout 5.1 -c:a libvorbis -q:a 3 six_48k_q3.ogg
    ffmpeg -f lavfi -i "aevalsrc=exprs='<chirp 8>':s=44100:d=0.4"  -channel_layout 7.1 -c:a libvorbis -q:a 4 eight_44k_q4.ogg
    ffmpeg -f lavfi -i "aevalsrc=exprs='<chirp 2>':s=44100:d=1.0"  -c:a vorbis -strict experimental -q:a 4 stereo_native.ogg
    ffmpeg -f lavfi -i "aevalsrc=exprs='<chirp 1>':s=44100:d=0.01" -c:a libvorbis -q:a 3 mono_tiny.ogg
    ffmpeg -f lavfi -i "anoisesrc=d=30:c=pink:r=44100:a=0.3:seed=7,aformat=channel_layouts=mono" -c:a libvorbis -q:a 0 mono_long.ogg
    ffmpeg -f lavfi -i "anoisesrc=d=3:c=pink:r=44100:a=0.3:seed=11,aformat=channel_layouts=mono" -c:a libvorbis -q:a 0 -page_duration 100000 paged.ogg

`mono_empty_page.ogg` is `paged.ogg` with 27 bytes put in before its last page: `OggS`, two
zero bytes, eight bytes of 255 (no sample position), the stream's serial number (bytes 14 to
17 of the file), and nine zero bytes (page number, checksum, and a count of no segments).
