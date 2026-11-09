use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(segments: &str, stdin: &[u8], options: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("segments.tsv");
    fs::write(&path, segments).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(options)
        .arg("--segments")
        .arg(&path)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    let bytes = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&bytes);
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    output
}

fn lines(output: &Output) -> Vec<String> {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout.clone()).unwrap().lines().map(str::to_owned).collect()
}

fn ascii(offset: u64, text: &str) -> String {
    format!("{offset:020}\tASCII\t{text}")
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

const CHUNK_OPTIONS: [&[&str]; 3] = [&["-c", "2"], &["-c", "6", "-j", "3"], &[]];

#[test]
fn strings_do_not_cross_a_segment_boundary() {
    for options in CHUNK_OPTIONS {
        let out = run("100\t8\t0\n5000\t8\t0\n", b"abcdefghijklmnop", options);
        assert_eq!(lines(&out), [ascii(100, "abcdefgh"), ascii(5000, "ijklmnop")], "{options:?}");
    }
}

/// An even run overlapped by a longer odd run that starts `shared` units
/// before its end. `[0, 0]` ends the even reading only, and the zero pair in
/// the middle ends the odd reading only.
fn overlapping_runs(pad: usize) -> (Vec<u8>, usize, usize) {
    let mut bytes = vec![0; pad + 4];
    let both = |units| [0x42, 0x4e].repeat(units);
    bytes.extend(both(50));
    bytes.extend([0x42, 0, 0, 0x4e]);
    let odd_start = bytes.len() - 1;
    bytes.extend(both(10));
    bytes.extend([0, 0]);
    bytes.extend(both(70));
    bytes.extend([0; 4]);
    (bytes, pad + 4, odd_start)
}

#[test]
fn a_longer_candidate_in_the_extension_does_not_remove_an_owned_string() {
    // `pad` swaps which parity is in the owned part.
    for pad in [0, 1] {
        let (bytes, owned_start, candidate) = overlapping_runs(pad);
        let options = ["-e", "utf16le", "-f", "printable", "-m", "3"];
        let label = "\tUTF16LE\t";
        for chunk in ["8", "auto"] {
            let options = [&options[..], &["-c", chunk]].concat();
            let len = bytes.len();
            let segments = format!("1000\t{candidate}\t{}\n", len - candidate);
            let out = lines(&run(&segments, &bytes, &options));
            assert_eq!(out.len(), 1, "pad={pad} -c {chunk}: {out:?}");
            let start = format!("{:020}{label}", 1000 + owned_start);
            assert!(out[0].starts_with(&start), "pad={pad} -c {chunk}: {out:?}");

            // Owned as well, the candidate is judged by length and wins.
            let whole = format!("1000\t{len}\t0\n");
            let out = lines(&run(&whole, &bytes, &options));
            assert_eq!(out.len(), 1, "pad={pad} -c {chunk}: {out:?}");
            let start = format!("{:020}{label}", 1000 + candidate);
            assert!(out[0].starts_with(&start), "pad={pad} -c {chunk}: {out:?}");
        }
    }
}

#[test]
fn a_string_starting_in_the_extension_is_not_output() {
    for options in CHUNK_OPTIONS {
        let out = run("10\t4\t12\n", b"\0\0\0\0\0\0\0\0string\0\0", options);
        assert_eq!(lines(&out), Vec::<String>::new(), "{options:?}");
    }
}

#[test]
fn a_string_starting_in_the_owned_part_runs_through_the_extension() {
    for options in CHUNK_OPTIONS {
        let out = run("1000\t4\t12\n", b"\0\0abcdefghij\0\0\0\0", options);
        assert_eq!(lines(&out), [ascii(1002, "abcdefghij")], "{options:?}");
    }
}

#[test]
fn the_extension_cuts_a_string_and_the_count_shows_it() {
    let out = run("0\t4\t4\n", b"\0abcdefg", &["--stats"]);
    assert_eq!(lines(&out), [ascii(1, "abcdefg")]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cut_strings: 1"));

    // Without an extension the owned part simply ends there.
    let out = run("0\t8\t0\n", b"\0abcdefg", &["--stats"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cut_strings: 0"));
}

#[test]
fn a_fragment_at_the_start_of_a_segment_is_output() {
    // F3 passes no bytes from before the segment, so nothing marks this as a
    // continuation, for any encoding.
    let out = run("9\t10\t0\n", b"continued\0", &[]);
    assert_eq!(lines(&out), [ascii(9, "continued")]);
    let cp932 = b"\x93\xfa\x96\x7b\x8c\xea\x83\x5c\0";
    let out = run("9\t9\t0\n", cp932, &["-e", "cp932"]);
    assert_eq!(lines(&out), [format!("{:020}\tCP932\t日本語ソ", 9)]);
}

#[test]
fn utf16_does_not_depend_on_the_parity_of_base_offset() {
    let mut bytes = utf16("even string\0");
    bytes.push(0);
    bytes.extend(utf16("odd string\0"));
    bytes.push(0);
    let len = bytes.len();
    for base in [1000, 1001] {
        for filter in ["utf16le-ascii", "utf16le"] {
            for chunk in ["2", "8", "auto"] {
                let segments = format!("{base}\t{len}\t0\n");
                let out = run(&segments, &bytes, &["-e", filter, "-c", chunk]);
                assert_eq!(
                    lines(&out),
                    [
                        format!("{base:020}\tUTF16LE\teven string"),
                        format!("{:020}\tUTF16LE\todd string", base + 25),
                    ],
                    "{filter} -c {chunk}"
                );
            }
        }
    }
}

#[test]
fn offsets_stay_sorted_across_many_small_segments() {
    let mut segments = String::new();
    let mut bytes = Vec::new();
    let mut expected = Vec::new();
    for i in 0..500u64 {
        let text = format!("segment {i}");
        segments.push_str(&format!("{}\t{}\t2\n", i * 100, text.len()));
        bytes.extend(text.as_bytes());
        bytes.extend([0, 0]);
        expected.push(ascii(i * 100, &text));
    }
    assert_eq!(lines(&run(&segments, &bytes, &["-j", "4"])), expected);
}

#[test]
fn a_declaration_that_does_not_fit_is_an_error() {
    for (segments, stdin, message) in [
        ("100\t4\t0\n50\t4\t0\n", &b"abcdefgh"[..], "before the end of the previous"),
        ("100\t4\t0\n103\t4\t0\n", b"abcdefgh", "before the end of the previous"),
        ("0\t4\t0\n", b"abc", "shorter than the declared"),
        ("0\t4\t4\n", b"abcdef", "shorter than the declared"),
        ("0\t4\t0\n", b"abcde", "longer than the declared"),
        ("0\t4\n", b"abcd", "three decimal numbers"),
        ("0x10\t4\t0\n", b"abcd", "three decimal numbers"),
        ("", b"", "no segments"),
    ] {
        let out = run(segments, stdin, &[]);
        assert_eq!(out.status.code(), Some(1), "{segments:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(message), "{segments:?}: {stderr}");
    }
}

#[test]
fn segments_need_standard_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, b"abcdef").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .arg("--segments")
        .arg(&input)
        .arg(&input)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

/// Runs on a file holding `bytes`.
fn run_file(bytes: &[u8], options: &[&str]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.bin");
    fs::write(&path, bytes).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pstrings")).args(options).arg(&path).output().unwrap();
    lines(&out)
}

/// An even run and an odd run of 113 units each, overlapping by 11.
fn tied_runs() -> Vec<u8> {
    let both = |units| [0x42, 0x4e].repeat(units);
    let mut bytes = vec![0; 4];
    bytes.extend(both(100));
    bytes.extend([0x42, 0, 0, 0x4e]);
    bytes.extend(both(11));
    bytes.extend([0, 0]);
    bytes.extend(both(100));
    bytes.extend([0; 4]);
    bytes
}

#[test]
fn a_tie_between_parities_goes_by_the_printed_offset() {
    let runs = tied_runs();
    let options = ["-e", "utf16le", "-f", "printable", "-m", "3"];
    for base in [1000, 1001] {
        let mut whole = vec![0; base];
        whole.extend(&runs);
        let expected = run_file(&whole, &options);
        assert_eq!(expected.len(), 1, "{expected:?}");
        // The run starting at an even offset in the file wins.
        let even_start = if base % 2 == 0 { 4 } else { 207 };
        assert!(expected[0].starts_with(&format!("{:020}\t", base + even_start)), "{expected:?}");
        for chunk in ["8", "auto"] {
            let segments = format!("{base}\t{}\t0\n", runs.len());
            let out = lines(&run(&segments, &runs, &[&options[..], &["-c", chunk]].concat()));
            assert_eq!(out, expected, "base={base} -c {chunk}");
        }
    }
}

#[test]
fn segments_cut_where_no_string_crosses_match_the_whole_input() {
    // Text readable at both parities, with candidates of equal length
    // overlapping, in pieces separated by runs of zeros. Each segment starts
    // inside such a run, at an odd offset.
    let piece = |seed: u8| -> Vec<u8> {
        let mut bytes = Vec::new();
        for i in 0..40u8 {
            let units = usize::from((seed.wrapping_mul(31) ^ i.wrapping_mul(17)) % 23 + 1);
            bytes.extend([0x42, 0x4e].repeat(units));
            bytes.extend(if (seed ^ i) % 3 == 0 { &[0x42, 0, 0, 0x4e][..] } else { &[0, 0][..] });
        }
        bytes
    };
    let options = ["-e", "utf16le", "-f", "printable", "-m", "3"];
    let mut whole = vec![0; 1];
    let mut stdin = Vec::new();
    let mut segments = String::new();
    for seed in 0..6 {
        whole.extend([0; 128]);
        let base = whole.len();
        assert_eq!(base % 2, 1);
        let mut segment = vec![0; 128];
        segment.extend(piece(seed));
        segment.extend([0; 128]);
        segments.push_str(&format!("{base}\t{}\t0\n", segment.len()));
        whole.extend(&segment);
        stdin.extend(segment);
        whole.extend([0; 128]);
    }
    // The same segments one byte further on start at even offsets.
    let shifted: String = segments
        .lines()
        .map(|line| {
            let (base, rest) = line.split_once('\t').unwrap();
            format!("{}\t{rest}\n", base.parse::<u64>().unwrap() + 1)
        })
        .collect();
    let expected = run_file(&whole, &options);
    let expected_shifted = run_file(&[&[0][..], &whole].concat(), &options);
    for chunk in ["8", "64", "auto"] {
        let options = [&options[..], &["-c", chunk, "-j", "4"]].concat();
        assert_eq!(lines(&run(&segments, &stdin, &options)), expected, "-c {chunk}");
        assert_eq!(lines(&run(&shifted, &stdin, &options)), expected_shifted, "-c {chunk}");
    }
}

#[test]
fn a_candidate_past_the_largest_offset_is_judged_without_overflow() {
    // The candidate in the extension would print past u64::MAX; it is not
    // output, but its parity is still decided.
    let mut bytes = vec![0; 6];
    bytes.extend(utf16("abcdef"));
    bytes.extend([0; 4]);
    let base = u64::MAX - 3;
    let segments = format!("{base}\t2\t{}\n", bytes.len() - 2);
    let out = run(&segments, &bytes, &["-e", "utf16le", "-m", "3"]);
    assert_eq!(lines(&out), Vec::<String>::new());
}
