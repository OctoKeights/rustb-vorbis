#!/usr/bin/env bash
# Checks the port against stb_vorbis itself: decodes every .ogg file under a folder with both,
# through every function they have (compare/dump.c and examples/dump.rs), and compares what
# they print, which has to be the same to the byte.
#
#   compare/compare.sh <folder holding stb_vorbis.c> <folder of .ogg files> [double|float|float-cpp] [jobs]
#
# double (the default), float or float-cpp: which stb_vorbis to compare with, see `Pi` in
# src/lib.rs. Needs a C compiler (cc, or $CC; for float-cpp a C++ one: c++, or $CXX) and cargo. What the two printed is left in $TMPDIR or /tmp,
# as rustb-vorbis-compare.c.txt and rustb-vorbis-compare.rust.txt.
set -euo pipefail

if [ $# -lt 2 ]; then
    sed -n '2,10p' "$0"
    exit 2
fi
stb=$1
sounds=$2
pi=${3:-double}
jobs=${4:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)}
here=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
out=${TMPDIR:-/tmp}

case $pi in
double) compiler=${CC:-cc}; define=; flag= ;;
float) compiler=${CC:-cc}; define=-DFLOAT_PI; flag=--float-pi ;;
float-cpp) compiler="${CXX:-c++} -x c++ -w"; define=-DFLOAT_PI; flag=--float-pi-cpp ;;
*) echo "double, float or float-cpp, not '$pi'" >&2; exit 2 ;;
esac

# (-ffp-contract=off: no fused multiply-add where the processor has it, which Rust never uses.)
$compiler -O2 -DNDEBUG -ffp-contract=off $define -I "$stb" "$here/compare/dump.c" -lm -o "$work/dump_c"
(cd "$here" && cargo build --quiet --release --example dump)
rust=$(cd "$here" && cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')/release/examples/dump

find "$sounds" -type f -iname '*.ogg' | LC_ALL=C sort > "$work/files"
count=$(wc -l < "$work/files")
if [ "$count" -eq 0 ]; then
    echo "no .ogg files under $sounds" >&2
    exit 2
fi
split -d -a 3 -n "l/$jobs" "$work/files" "$work/part."

# run <program and its flags> <suffix>: the program over every part of the list at once.
run() {
    local suffix=$1
    shift
    for part in "$work"/part.[0-9][0-9][0-9]; do
        (tr '\n' '\0' < "$part" | xargs -0 -r -n 200 "$@" > "$part.$suffix") &
    done
    wait
    cat "$work"/part.[0-9][0-9][0-9]."$suffix"
}
run c "$work/dump_c" > "$out/rustb-vorbis-compare.c.txt"
run rust "$rust" $flag > "$out/rustb-vorbis-compare.rust.txt"

lines=$(wc -l < "$out/rustb-vorbis-compare.c.txt")
if cmp -s "$out/rustb-vorbis-compare.c.txt" "$out/rustb-vorbis-compare.rust.txt"; then
    echo "the same: $count files, $lines lines ($pi pi)"
else
    echo "DIFFERENT ($pi pi): $count files" >&2
    diff "$out/rustb-vorbis-compare.c.txt" "$out/rustb-vorbis-compare.rust.txt" | head -20 >&2
    exit 1
fi
