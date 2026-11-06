use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pstrings")).args(args).output().unwrap()
}

fn lines(output: &Output) -> Vec<String> {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout.clone()).unwrap().lines().map(str::to_owned).collect()
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn write_input(dir: &Path, bytes: &[u8]) -> String {
    let path = dir.join("input.bin");
    fs::write(&path, bytes).unwrap();
    path.to_str().unwrap().to_owned()
}

/// Extracts `[0, len)` as consecutive ranges cut at `cuts`.
fn split_scan(path: &str, len: u64, cuts: &[u64], options: &[&str]) -> Vec<String> {
    let mut edges = vec![0];
    edges.extend_from_slice(cuts);
    edges.push(len);
    let mut result = Vec::new();
    for edge in edges.windows(2).filter(|edge| edge[0] < edge[1]) {
        let offset = edge[0].to_string();
        let length = (edge[1] - edge[0]).to_string();
        let args = [options, &["--offset", &offset, "--length", &length, path]].concat();
        result.extend(lines(&run(&args)));
    }
    result
}

fn assert_every_split_matches(bytes: &[u8], options: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), bytes);
    let whole = lines(&run(&[options, &[path.as_str()]].concat()));
    assert!(!whole.is_empty());
    for cut in 1..bytes.len() as u64 {
        assert_eq!(split_scan(&path, bytes.len() as u64, &[cut], options), whole, "cut={cut} {options:?}");
    }
}

#[test]
fn splitting_anywhere_matches_the_whole_file_ascii_and_utf16() {
    let mut bytes = b"\0\0ascii string\0".to_vec();
    bytes.extend(utf16("\0odd position\0"));
    bytes.push(0);
    bytes.extend(utf16("even position\0"));
    assert_eq!(bytes.len() % 2, 0);
    assert_every_split_matches(&bytes, &["-e", "ascii,utf16le-ascii"]);
    assert_every_split_matches(&bytes, &["-e", "ascii,utf16le-ascii", "-c", "6", "-j", "3"]);
}

#[test]
fn splitting_anywhere_matches_the_whole_file_full_utf16() {
    for pad in 0..2 {
        let mut bytes = vec![0; pad + 2];
        bytes.extend(utf16("ABC漢字DEFあいう𠀀𠀀\0\0"));
        bytes.extend(utf16("かなかな漢字\0"));
        assert_every_split_matches(
            &bytes,
            &["-e", "utf16le", "-f", "ascii,kanji,hiragana,katakana,kanji-ext-b", "-c", "8"],
        );
    }
}

#[test]
fn splitting_anywhere_matches_the_whole_file_utf8_and_cp932() {
    let mut bytes = "\0日本語の文字列\0abc€𠀀def\0".as_bytes().to_vec();
    assert_every_split_matches(&bytes, &["-e", "utf8"]);
    bytes = b"\0\x93\xfa\x96\x7b\x8c\xea\x83\x5c\x83\x5cabc\0\xb1\xb2\xb3\xb4\0".to_vec();
    assert_every_split_matches(&bytes, &["-e", "cp932"]);
    assert_every_split_matches(&bytes, &["-e", "cp932", "-c", "3"]);
}

/// A small deterministic generator: `pstrings` has no dependency to reach for.
struct Rng(u64);

impl Rng {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }
}

/// Noise that a broad filter reads as text at both parities, so candidates
/// of opposite parity overlap and the longer one has to win across a cut.
fn overlapping_candidates(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        match rng.next(5) {
            0 => bytes.extend([0, 0, 0]),
            1 => bytes.extend(std::iter::repeat_with(|| 0x41).take(rng.next(30) as usize)),
            2 => bytes.extend((0..rng.next(40)).flat_map(|i| [0x41 + (i % 26) as u8, 0])),
            3 => bytes.extend((0..rng.next(40)).map(|i| if i % 2 == 0 { 0x42 } else { 0x4e })),
            _ => bytes.extend((0..rng.next(40)).map(|i| if i % 3 == 0 { 0x30 } else { 0x4e })),
        }
    }
    bytes.truncate(len);
    bytes
}

#[test]
fn overlap_resolution_is_the_same_on_both_sides_of_a_cut() {
    let filters = "printable";
    let options = ["-e", "utf16le", "-f", filters, "-m", "3", "-c", "64"];
    let dir = tempfile::tempdir().unwrap();
    let mut rng = Rng(0x5eed);
    for _ in 0..40 {
        let len = 600 + rng.next(400) as usize;
        let bytes = overlapping_candidates(&mut rng, len);
        let path = write_input(dir.path(), &bytes);
        let whole = lines(&run(&[&options[..], &[path.as_str()]].concat()));
        let mut cuts = BTreeSet::new();
        for _ in 0..1 + rng.next(6) {
            cuts.insert(1 + rng.next(len as u64 - 1));
        }
        let cuts: Vec<u64> = cuts.into_iter().collect();
        assert_eq!(split_scan(&path, len as u64, &cuts, &options), whole, "cuts={cuts:?}");
    }
}

/// Bytes that read as text at both parities, `units` code units long.
fn both_parities(units: usize) -> Vec<u8> {
    [0x42, 0x4e].repeat(units)
}

/// Ends the odd reading only: U+0000 to it, but "B" and U+4E00 to the even one.
const ODD_BREAK: [u8; 4] = [0x42, 0, 0, 0x4e];
/// Ends the even reading only: U+0000 to it, but "N" and U+4200 to the odd one.
const EVEN_BREAK: [u8; 2] = [0, 0];

/// An even run of 2 + `before` + `shared` units, overlapped from its last
/// `shared` units by an odd run that goes on for `after` units more.
fn overlapping_runs(before: usize, shared: usize, after: usize) -> (Vec<u8>, u64) {
    let mut bytes = vec![0; 4];
    bytes.extend(both_parities(before));
    bytes.extend(ODD_BREAK);
    let odd_start = bytes.len() as u64 - 1;
    bytes.extend(both_parities(shared));
    bytes.extend(EVEN_BREAK);
    bytes.extend(both_parities(after));
    bytes.extend([0; 4]);
    (bytes, odd_start)
}

const OVERLAP_OPTIONS: [&str; 6] = ["-e", "utf16le", "-f", "printable", "-m", "3"];

#[test]
fn a_string_mostly_before_the_range_still_beats_a_candidate_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let (bytes, odd_start) = overlapping_runs(200, 20, 100);
    let path = write_input(dir.path(), &bytes);
    let whole = lines(&run(&[&OVERLAP_OPTIONS[..], &[path.as_str()]].concat()));
    assert_eq!(whole.len(), 1, "only the even run survives: {whole:?}");
    assert!(whole[0].starts_with("00000000000000000004\t"));
    for cut in odd_start - 2..odd_start + 8 {
        let split = split_scan(&path, bytes.len() as u64, &[cut], &OVERLAP_OPTIONS);
        assert_eq!(split, whole, "cut={cut}");
    }
}

#[test]
fn a_candidate_past_the_limit_still_beats_a_string_in_the_range() {
    let dir = tempfile::tempdir().unwrap();
    let (bytes, odd_start) = overlapping_runs(50, 10, 70);
    let path = write_input(dir.path(), &bytes);
    let whole = lines(&run(&[&OVERLAP_OPTIONS[..], &[path.as_str()]].concat()));
    assert_eq!(whole.len(), 1, "only the odd run survives: {whole:?}");
    assert!(whole[0].starts_with(&format!("{odd_start:020}\t")));
    // Every string fits the limit, but the odd run ends beyond what the range
    // owning the even run may extend to.
    let options = [&OVERLAP_OPTIONS[..], &["--extend-limit", "170"]].concat();
    assert_eq!(split_scan(&path, bytes.len() as u64, &[6], &options), whole);
}

#[test]
fn a_tie_between_parities_goes_the_same_way_in_a_range() {
    let dir = tempfile::tempdir().unwrap();
    // Both runs are 113 units, and far enough in that the range starts
    // reading partway through the file, at a position derived from `cut`.
    let (runs, odd_start) = overlapping_runs(100, 11, 100);
    let pad = 70_000;
    let mut bytes = vec![0; pad];
    bytes.extend(runs);
    let path = write_input(dir.path(), &bytes);
    let whole = lines(&run(&[&OVERLAP_OPTIONS[..], &[path.as_str()]].concat()));
    assert_eq!(whole.len(), 1, "the even run wins the tie: {whole:?}");
    assert!(whole[0].starts_with(&format!("{:020}\t", pad + 4)));
    for cut in pad as u64 + odd_start - 3..pad as u64 + odd_start + 3 {
        let split = split_scan(&path, bytes.len() as u64, &[cut], &OVERLAP_OPTIONS);
        assert_eq!(split, whole, "cut={cut}");
    }
}

#[test]
fn every_range_keeps_file_offsets_and_starts_in_the_range() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), b"\0first string\0\0second string\0\0third string\0");
    let out = lines(&run(&["--offset", "0xe", "--length", "2", &path]));
    assert_eq!(out, ["00000000000000000015\tASCII\tsecond string"]);
}

#[test]
fn a_range_reads_past_its_end_but_not_past_the_limit() {
    let mut bytes = b"\0".to_vec();
    bytes.extend([b'x'; 100]);
    bytes.push(0);
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), &bytes);

    let out = lines(&run(&["--offset", "1", "--length", "10", &path]));
    assert_eq!(out, [format!("{:020}\tASCII\t{}", 1, "x".repeat(100))]);

    let cut = run(&["--offset", "1", "--length", "10", "--extend-limit", "20", "--stats", &path]);
    assert_eq!(lines(&cut), [format!("{:020}\tASCII\t{}", 1, "x".repeat(30))]);
    assert!(String::from_utf8_lossy(&cut.stderr).contains("cut_strings: 1"));

    let rest = run(&["--offset", "31", "--extend-limit", "20", &path]);
    assert_eq!(lines(&rest), Vec::<String>::new());
}

#[test]
fn the_limit_cuts_full_utf16_between_characters() {
    let mut bytes = vec![0, 0];
    bytes.extend(utf16("漢字漢字𠀀漢字漢字\0"));
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), &bytes);
    let options = ["-e", "utf16le", "-f", "kanji,kanji-ext-b", "--offset", "2", "--length", "2"];
    // The limit falls inside the surrogate pair, which is left out.
    let out = run(&[&options[..], &["--extend-limit", "8", "--stats", &path]].concat());
    assert_eq!(lines(&out), [format!("{:020}\tUTF16LE\t漢字漢字", 2)]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cut_strings: 1"));
}

#[test]
fn the_limit_counts_characters_correctly_in_mixed_width_text() {
    let mut bytes = vec![0, 0];
    bytes.extend(utf16(&format!("{}{}\0", "漢".repeat(22000), "a".repeat(10000))));
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), &bytes);
    let options = ["-e", "utf16le", "-f", "ascii,kanji", "-j", "1", "--offset", "2", "--length", "2"];
    let out = lines(&run(&[&options[..], &["--extend-limit", "45998", &path]].concat()));
    let expected = format!("{}{}", "漢".repeat(22000), "a".repeat(1000));
    assert_eq!(out, [format!("{:020}\tUTF16LE\t{expected}", 2)]);
}

#[test]
fn a_full_utf16_string_ending_inside_the_limit_is_not_cut() {
    let dir = tempfile::tempdir().unwrap();
    for (text, filter, limit) in [
        // The string ends exactly at the stop, and just before it.
        ("abcd\0\0", "ascii", "6"),
        ("abcd\0\0", "ascii", "8"),
        ("𠀀𠀀\0\0", "kanji-ext-b", "6"),
        ("𠀀𠀀\0\0", "kanji-ext-b", "8"),
    ] {
        let path = write_input(dir.path(), &utf16(text));
        let options = ["-e", "utf16le", "-f", filter, "-m", "2", "--offset", "0", "--length", "2"];
        let out = run(&[&options[..], &["--extend-limit", limit, "--stats", &path]].concat());
        assert_eq!(lines(&out).len(), 1, "{text:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("cut_strings: 0"), "{text:?} limit {limit}: {stderr}");
    }
}

#[test]
fn a_full_utf16_string_running_past_the_limit_is_cut() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), &utf16("abcdefgh\0\0"));
    let options = ["-e", "utf16le", "--offset", "0", "--length", "2", "--extend-limit", "8", "--stats"];
    let out = run(&[&options[..], &[path.as_str()]].concat());
    assert_eq!(lines(&out), [format!("{:020}\tUTF16LE\tabcde", 0)]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cut_strings: 1"));
}

#[test]
fn a_string_ending_at_the_end_of_the_file_is_not_cut() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), b"\0abcdefgh");
    let out = run(&["--offset", "1", "--length", "2", "--extend-limit", "6", "--stats", &path]);
    assert_eq!(lines(&out), [format!("{:020}\tASCII\tabcdefgh", 1)]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cut_strings: 0"));
}

#[test]
fn the_end_of_the_file_stops_a_range_and_bad_ranges_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_input(dir.path(), b"\0abcdefgh\0");
    assert_eq!(lines(&run(&["--length", "1T", &path])).len(), 1);
    assert_eq!(run(&["--length", "0", &path]).status.code(), Some(1));
    assert_eq!(run(&["--offset", "10", &path]).status.code(), Some(1));
    assert_eq!(run(&["--offset", "1", "-"]).status.code(), Some(1));
}
