use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn sample() -> Vec<u8> {
    let mut bytes = b"\0\0plain ascii\0".to_vec();
    bytes.extend(utf16("\0abcd漢字かな\0\0"));
    bytes.push(0);
    bytes.extend(utf16("odd position\0"));
    bytes.extend("\0日本語の文字列\0".as_bytes());
    bytes
}

fn pstrings(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(args)
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    assert!(status.success());
}

fn assert_each_output_matches_a_run_of_its_own(dir: &Path, input: &str, range: &[&str]) {
    let encodings = ["ascii", "utf16le", "utf16le-ascii", "utf8"];
    let filters = "ascii,kanji,hiragana";
    let template = dir.join("out-{encoding}.tsv");
    let joined = encodings.join(",");
    let common = ["-f", filters, "-c", "6", "-j", "3"];
    let args = ["-e", joined.as_str(), "-o", template.to_str().unwrap(), input];
    pstrings(&[&common[..], range, &args].concat());

    for encoding in encodings {
        let alone = dir.join("alone.tsv");
        let args = ["-e", encoding, "-o", alone.to_str().unwrap(), input];
        pstrings(&[&common[..], range, &args].concat());
        let path = dir.join(format!("out-{encoding}.tsv"));
        assert_eq!(fs::read(path).unwrap(), fs::read(&alone).unwrap(), "{encoding}");
    }
}

#[test]
fn each_output_is_what_that_encoding_gives_alone() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, sample()).unwrap();
    let input = input.to_str().unwrap();
    assert_each_output_matches_a_run_of_its_own(dir.path(), input, &[]);
    assert_each_output_matches_a_run_of_its_own(dir.path(), input, &["--offset", "5", "--length", "30"]);
}

#[test]
fn outputs_keep_lines_the_shared_label_would_merge() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    let bytes = utf16("same text\0");
    fs::write(&input, &bytes).unwrap();
    let template = dir.path().join("{encoding}.tsv");
    pstrings(&["-e", "utf16le,utf16le-ascii", "-o", template.to_str().unwrap(), input.to_str().unwrap()]);
    let expected = format!("{:020}\tUTF16LE\tsame text", 0);
    for name in ["utf16le.tsv", "utf16le-ascii.tsv"] {
        assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap().trim_end(), expected);
    }
}

#[test]
fn an_encoding_without_matches_still_gets_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, b"\0plain ascii\0").unwrap();
    let template = dir.path().join("{encoding}.tsv");
    pstrings(&["-o", template.to_str().unwrap(), input.to_str().unwrap()]);
    assert!(!fs::read(dir.path().join("ascii.tsv")).unwrap().is_empty());
    assert!(fs::read(dir.path().join("utf16le-ascii.tsv")).unwrap().is_empty());
}

#[test]
fn a_template_cannot_overwrite_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("ascii.bin");
    fs::write(&input, b"\0plain ascii\0").unwrap();
    let template = dir.path().join("{encoding}.bin");
    let status = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(["-e", "ascii", "-o", template.to_str().unwrap(), input.to_str().unwrap()])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert_eq!(fs::read(&input).unwrap(), b"\0plain ascii\0");
}
