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
