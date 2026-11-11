//! The output depends only on the input and the options that select what
//! is extracted (-e, -f, -m, the range options and --line-ending), never on
//! how the work is split (-j, -c), where temporary files go, or the
//! environment of the run.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const ENCODINGS: &str = "ascii,utf16le-ascii,utf16le";

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*: deterministic, so every run sees the same input.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Random bytes with strings planted in them: ASCII, and UTF-16LE at even
/// and odd positions, many of them across a multiple of 4 KiB, so that
/// every chunk size used below has strings crossing its boundaries.
fn input(len: usize) -> Vec<u8> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
    let mut at = 0usize;
    while at + 4096 < len {
        let boundary = at + 4096;
        let text: String = (0..4 + rng.below(40)).map(|_| (b'a' + rng.below(26) as u8) as char).collect();
        let planted: Vec<u8> = match rng.below(3) {
            0 => text.into_bytes(),
            _ => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        };
        // Start the string a few bytes before the boundary, at either parity.
        let start = boundary - 1 - rng.below(planted.len() as u64) as usize;
        let end = (start + planted.len()).min(len);
        bytes[start..end].copy_from_slice(&planted[..end - start]);
        if end < len {
            bytes[end] = 0;
        }
        at = boundary;
    }
    bytes
}

fn extract(args: &[&str], stdin: Option<&[u8]>, temp_dir: &Path, env: &[(&str, &str)]) -> Vec<u8> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pstrings"))
        .args(["-e", ENCODINGS, "--line-ending", "lf", "--temp-dir"])
        .arg(temp_dir)
        .args(args)
        .envs(env.iter().copied())
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let writer = stdin.map(|bytes| {
        let mut pipe = child.stdin.take().unwrap();
        let bytes = bytes.to_vec();
        std::thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        })
    });
    let output = child.wait_with_output().unwrap();
    if let Some(writer) = writer {
        writer.join().unwrap();
    }
    assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

fn cpus() -> String {
    std::thread::available_parallelism().map_or(1, |n| n.get()).to_string()
}

/// Asserts that every -j/-c combination, temp dir and environment gives
/// the same bytes as the first run.
fn assert_reproducible(input_args: &[&str], stdin: Option<&[u8]>, chunk_sizes: &[&str]) {
    let temp_a = tempfile::tempdir().unwrap();
    let temp_b = tempfile::tempdir().unwrap();
    let reference = extract(&[input_args, &["-j", "1", "-c", "auto"]].concat(), stdin, temp_a.path(), &[]);
    assert!(reference.len() > 1000, "too little output to compare");

    let cpus = cpus();
    for jobs in ["1", "4", cpus.as_str()] {
        for chunk in chunk_sizes {
            let args = [input_args, &["-j", jobs, "-c", chunk]].concat();
            let output = extract(&args, stdin, temp_b.path(), &[]);
            assert!(output == reference, "-j {jobs} -c {chunk}: output differs");
        }
    }

    let env = [("LC_ALL", "ja_JP.UTF-8"), ("LANG", "ja_JP.UTF-8"), ("TZ", "Asia/Tokyo")];
    let args = [input_args, &["-j", "4", "-c", "auto"]].concat();
    assert!(extract(&args, stdin, temp_a.path(), &env) == reference, "environment changed the output");
}

fn assert_file_and_segments_reproducible(len: usize, chunk_sizes: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.bin");
    let bytes = input(len);
    fs::write(&path, &bytes).unwrap();
    let path = path.to_str().unwrap();

    assert_reproducible(&[path], None, chunk_sizes);
    let third = (len / 3) as u64;
    assert_reproducible(
        &["--offset", &third.to_string(), "--length", &third.to_string(), path],
        None,
        chunk_sizes,
    );

    // Three segments, owned parts in ascending order with gaps, the first
    // two with an extension.
    let segments = dir.path().join("segments.tsv");
    let (a, b, c) = (third - 4096, third - 1, len as u64 - 2 * third);
    fs::write(&segments, format!("0\t{a}\t4096\n{}\t{b}\t1\n{}\t{c}\t0\n", 2 * third, 4 * third)).unwrap();
    assert_eq!(a + 4096 + b + 1 + c, len as u64);
    assert_reproducible(&["--segments", segments.to_str().unwrap(), "-"], Some(&bytes), chunk_sizes);
}

#[test]
fn output_does_not_depend_on_threads_chunks_temp_dir_or_environment() {
    assert_file_and_segments_reproducible(2 << 20, &["4K", "65538", "1M", "auto"]);
}

/// The same check at the scale of a real image. Run with
/// `cargo test --release --test reproducibility -- --ignored`.
#[test]
#[ignore = "several hundred MiB; run explicitly"]
fn large_output_does_not_depend_on_threads_chunks_temp_dir_or_environment() {
    assert_file_and_segments_reproducible(384 << 20, &["1M", "16M", "64M", "auto"]);
}
