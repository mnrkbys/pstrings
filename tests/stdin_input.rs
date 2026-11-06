use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(args)
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

/// Text that crosses small chunks in every encoding the test selects.
fn sample() -> Vec<u8> {
    let mut bytes = b"\0\0plain ascii text\0".to_vec();
    bytes.extend("\0odd utf16 文字列です\0\0".encode_utf16().flat_map(u16::to_le_bytes));
    bytes.push(0);
    bytes.extend("even utf16 かな\0".encode_utf16().flat_map(u16::to_le_bytes));
    bytes.extend("\0utf8 日本語の文字列\0".as_bytes());
    bytes.extend(b"\0cp932 \x93\xfa\x96\x7b\x8c\xea\x83\x5c\x83\x5c\0");
    bytes.extend("𠀀𠀀𠀀𠀀\0".encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

#[test]
fn a_stream_gives_the_output_of_a_file_with_the_same_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.bin");
    let bytes = sample();
    fs::write(&path, &bytes).unwrap();
    let path = path.to_str().unwrap();

    for encodings in ["ascii,utf16le-ascii", "utf8", "cp932", "utf16le"] {
        let filters = "ascii,kanji,hiragana,katakana,kanji-ext-b";
        let common = ["-e", encodings, "-f", filters, "-j", "3"];
        let expected = run(&[&common[..], &[path]].concat(), b"");
        assert!(expected.status.success());
        assert!(!expected.stdout.is_empty(), "{encodings}");
        for chunk in ["2", "4", "6", "10", "64", "auto"] {
            let actual = run(&[&common[..], &["-c", chunk, "-"]].concat(), &bytes);
            assert!(actual.status.success(), "{}", String::from_utf8_lossy(&actual.stderr));
            assert_eq!(actual.stdout, expected.stdout, "{encodings} -c {chunk}");
        }
    }
}

#[test]
fn a_stream_longer_than_the_workers_can_hold_stays_in_order() {
    let mut bytes = Vec::new();
    for i in 0..20_000 {
        bytes.extend(format!("line number {i}\0").as_bytes());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.bin");
    fs::write(&path, &bytes).unwrap();
    let expected = run(&[path.to_str().unwrap()], b"");
    let actual = run(&["-c", "4K", "-j", "4", "-"], &bytes);
    assert!(actual.status.success());
    assert_eq!(actual.stdout, expected.stdout);
}

#[test]
fn an_empty_stream_is_rejected_like_an_empty_file() {
    let output = run(&["-"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("empty input"));
}

#[cfg(unix)]
#[test]
fn a_read_error_fails_the_run() {
    // A directory opens fine but every read of it fails.
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .arg("-")
        .stdin(fs::File::open(dir.path()).unwrap())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[cfg(unix)]
#[test]
fn a_termination_request_ends_a_stalled_stream_and_leaves_no_temporary_files() {
    use std::time::{Duration, Instant};

    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(["-c", "4K", "-j", "2", "--temp-dir"])
        .arg(temp.path())
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Some complete chunks, then a pipe that stays open with nothing to read.
    let mut pipe = child.stdin.take().unwrap();
    pipe.write_all(&b"some text\0".repeat(5000)).unwrap();
    std::thread::sleep(Duration::from_millis(500));

    let started = Instant::now();
    let kill = Command::new("kill").arg("-TERM").arg(child.id().to_string()).status().unwrap();
    assert!(kill.success());
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "still running");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(1));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    drop(pipe);
}
