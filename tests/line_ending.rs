use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn sample() -> Vec<u8> {
    let mut bytes = b"\0\0plain ascii\0more text here\0".to_vec();
    bytes.extend(utf16("\0even position\0"));
    bytes.push(0);
    bytes.extend(utf16("odd position\0"));
    bytes.extend(b"\0trailing ascii\0");
    bytes
}

fn run(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(bytes) = stdin {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    child.wait_with_output().unwrap()
}

/// Runs with each `--line-ending` value (and without one) and returns the
/// bytes that `read` collects from the run, in the order none, lf, crlf,
/// native.
fn outputs(args: &[&str], stdin: Option<&[u8]>, read: &dyn Fn(Output) -> Vec<u8>) -> [Vec<u8>; 4] {
    let with = |value: Option<&str>| {
        let mut all = args.to_vec();
        if let Some(value) = value {
            all.extend(["--line-ending", value]);
        }
        let output = run(&all, stdin);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        read(output)
    };
    [with(None), with(Some("lf")), with(Some("crlf")), with(Some("native"))]
}

fn assert_line_endings(args: &[&str], stdin: Option<&[u8]>, read: &dyn Fn(Output) -> Vec<u8>) {
    let [default, lf, crlf, native] = outputs(args, stdin, read);
    assert!(!lf.is_empty(), "{args:?}: no output");
    assert!(!lf.contains(&b'\r'), "{args:?}: lf output has a CR");
    assert!(lf.ends_with(b"\n"), "{args:?}");
    let lf_text = String::from_utf8(lf.clone()).unwrap();
    assert_eq!(String::from_utf8(crlf.clone()).unwrap(), lf_text.replace('\n', "\r\n"), "{args:?}");
    let platform = if cfg!(windows) { &crlf } else { &lf };
    assert_eq!(&native, platform, "{args:?}: native");
    assert_eq!(&default, platform, "{args:?}: default");
}

fn stdout(output: Output) -> Vec<u8> {
    output.stdout
}

#[test]
fn every_input_mode_ends_lines_as_selected() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    let bytes = sample();
    fs::write(&input, &bytes).unwrap();
    let input = input.to_str().unwrap();
    let segments = dir.path().join("segments.tsv");
    fs::write(&segments, format!("0\t20\t4\n100\t{}\t0\n", bytes.len() - 24)).unwrap();
    let segments = segments.to_str().unwrap();

    assert_line_endings(&[input], None, &stdout);
    assert_line_endings(&["-s", input], None, &stdout);
    assert_line_endings(&["-c", "6", "-j", "3", input], None, &stdout);
    assert_line_endings(&["-"], Some(&bytes), &stdout);
    assert_line_endings(&["--offset", "4", "--length", "40", input], None, &stdout);
    assert_line_endings(&["--segments", segments, "-"], Some(&bytes), &stdout);
}

fn read_files(paths: &[&Path]) -> impl Fn(Output) -> Vec<u8> + use<> {
    let paths: Vec<_> = paths.iter().map(|path| path.to_path_buf()).collect();
    move |output| {
        assert!(output.stdout.is_empty());
        paths.iter().flat_map(|path| fs::read(path).unwrap()).collect()
    }
}

#[test]
fn output_files_end_lines_as_selected() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, sample()).unwrap();
    let input = input.to_str().unwrap();

    let single = dir.path().join("out.tsv");
    let read = read_files(&[&single]);
    assert_line_endings(&["-o", single.to_str().unwrap(), input], None, &read);
    assert_line_endings(&["-s", "-o", single.to_str().unwrap(), input], None, &read);

    let template = dir.path().join("out-{encoding}.tsv");
    let ascii = dir.path().join("out-ascii.tsv");
    let utf16 = dir.path().join("out-utf16le-ascii.tsv");
    let read = read_files(&[&ascii, &utf16]);
    assert_line_endings(&["-o", template.to_str().unwrap(), input], None, &read);
    assert!(!fs::read(&utf16).unwrap().is_empty());
}

#[test]
fn an_unknown_line_ending_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, sample()).unwrap();
    for value in ["cr", "LF", "", "unix"] {
        let output = run(&["--line-ending", value, input.to_str().unwrap()], None);
        assert!(!output.status.success(), "{value:?}");
        assert!(output.stdout.is_empty(), "{value:?}");
    }
}
