use std::fs;
use std::process::{Command, Output};

fn run(bytes: &[u8], options: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, bytes).unwrap();
    Command::new(env!("CARGO_BIN_EXE_pstrings")).args(options).arg(&input).output().unwrap()
}

fn lines(output: &Output) -> Vec<String> {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .replace("\r\n", "\n")
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect()
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

const BOTH: [&str; 4] = ["-e", "utf16le,utf16le-ascii", "-f", "ascii,hiragana,kanji"];

#[test]
fn identical_lines_are_merged_at_any_chunk_size() {
    let mut bytes = vec![0, 0];
    bytes.extend(utf16("same text\0\0"));
    bytes.extend(utf16("more text\0"));
    let expected = vec![
        format!("{:020}\tUTF16LE\tsame text\n", 2),
        format!("{:020}\tUTF16LE\tmore text\n", 24),
    ];
    for chunk in ["2", "4", "6", "8", "10", "auto"] {
        let out = run(&bytes, &[&BOTH[..], &["-c", chunk, "-j", "3"]].concat());
        assert_eq!(lines(&out), expected, "-c {chunk}");
    }
}

#[test]
fn different_strings_at_one_offset_are_both_kept() {
    let mut bytes = vec![0, 0];
    bytes.extend(utf16("abcd漢字\0"));
    let out = run(&bytes, &BOTH);
    assert_eq!(
        lines(&out),
        [format!("{:020}\tUTF16LE\tabcd\n", 2), format!("{:020}\tUTF16LE\tabcd漢字\n", 2)]
    );
}

#[test]
fn string_only_output_judges_lines_by_their_offsets() {
    let mut bytes = utf16("same text\0\0");
    bytes.extend(utf16("same text\0"));
    let out = run(&bytes, &[&BOTH[..], &["-s"]].concat());
    assert_eq!(lines(&out), ["same text\n", "same text\n"]);
}

#[test]
fn no_record_is_only_a_line_break() {
    // CJK code units whose low byte is a line break, which `utf16le-ascii`
    // once took for the character itself.
    let mut bytes = utf16("漊漍\0\0");
    bytes.extend(utf16("かなかなかな\0"));
    for chunk in ["2", "4", "4096"] {
        let out = run(&bytes, &["-e", "utf16le,utf16le-ascii", "-f", "ascii,hiragana,kanji", "-c", chunk, "-j", "4"]);
        for line in lines(&out) {
            assert!(!line.trim_end_matches(['\r', '\n']).is_empty(), "{line:?}");
            assert_eq!(line.split('\t').count(), 3, "{line:?}");
        }
    }
}

#[test]
fn a_long_spilled_string_is_merged_too() {
    let text = "long text ".repeat(1024 * 1024);
    let mut bytes = utf16(&text);
    bytes.extend([0, 0]);
    let out = run(&bytes, &["-e", "utf16le,utf16le-ascii", "-c", "1M", "-j", "2"]);
    assert_eq!(lines(&out), [format!("{:020}\tUTF16LE\t{text}\n", 0)]);
}

#[test]
fn a_cut_string_is_merged_and_counted_once() {
    let mut bytes = vec![0, 0];
    bytes.extend(utf16("abcdefghij\0"));
    let options = [&BOTH[..], &["--offset", "2", "--length", "2", "--extend-limit", "8", "--stats"]].concat();
    let out = run(&bytes, &options);
    assert_eq!(lines(&out), [format!("{:020}\tUTF16LE\tabcde\n", 2)]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cut_strings: 1"));
}
