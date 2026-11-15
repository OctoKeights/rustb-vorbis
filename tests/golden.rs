//! The port against stb_vorbis itself: `tests/data/expected-*.txt` are what `compare/dump.c`
//! printed for the sounds in `tests/data` with the C library, and the port has to print the same.

#[allow(dead_code)]
#[path = "../examples/dump.rs"]
mod dump;

use rustb_vorbis::Pi;
use std::path::Path;

fn sounds() -> Vec<String> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut paths: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ogg"))
        .map(|path| path.to_str().unwrap().to_string())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8);
    paths
}

fn check(pi: Pi, expected: &str) {
    let mut out = Vec::new();
    for path in sounds() {
        dump::dump(&path, pi, &mut out);
    }
    let out = String::from_utf8(out).unwrap();
    for (line, (ours, theirs)) in out.lines().zip(expected.lines()).enumerate() {
        assert_eq!(ours, theirs, "line {}", line + 1);
    }
    assert_eq!(out.lines().count(), expected.lines().count());
}

#[test]
fn the_same_as_stb_vorbis_with_the_c_librarys_pi() {
    check(Pi::Double, include_str!("data/expected-double.txt"));
}

#[test]
fn the_same_as_stb_vorbis_with_its_own_pi() {
    check(Pi::Float, include_str!("data/expected-float.txt"));
}
