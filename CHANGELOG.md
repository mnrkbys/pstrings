# Changelog

All notable changes to this fork of [pstrings](https://github.com/herosi/pstrings)
are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow calendar
versioning, `YYYY.M.N` (`N` counts releases within the month, from 0), and are a
separate line from upstream's 0.1.x releases. The history starts from upstream
v0.1.46.

A release's date is filled in when it is tagged.

## [2026.11.0] - Unreleased

The first release of the fork. It reads from pipes, extracts parts of an input
without copying them out, and fixes output that could depend on the chunk size,
lose strings, or damage the input.

### Added

- `-` reads the input from standard input, including a pipe. Each worker holds
  one chunk of the input in memory while it scans it (16 MiB with the default
  `--chunk-size auto`), so the input held at once is at most the number of
  workers times the chunk size, however long the stream is. The input is never
  written to temporary files. The output is the same as for a file holding the
  same bytes.
- `--offset` and `--length` extract the strings starting in a range of a file.
  Printed offsets stay offsets in the file. Splitting a file into consecutive
  ranges gives the same strings as extracting it at once.
- `--extend-limit` (default 64 KiB) sets how far a string starting in a range may
  run past its end. Strings stopped there are cut.
- `--segments` describes standard input as a series of segments from unrelated
  places, each with its offset, an owned part, and an extension that strings may
  run into. Strings never cross from one segment into the next.
- `-o` with `{encoding}` in the path writes one output file per `-e` encoding,
  each as it would be if that encoding were extracted alone.
- `--stats` reports `cut_strings`, the number of strings cut by `--extend-limit`
  or a segment's end.
- Ctrl+C and SIGTERM end a run promptly, even when the input or the output is
  stalled, and remove the temporary directory with the strings extracted so far.

### Changed

- Version numbers are calendar based, starting at 2026.11.0.

### Fixed

- Writing the output over the input file, also through a symbolic or hard link,
  is refused before the file is truncated.
- Output stays sorted by offset when strings of several encodings span chunk
  boundaries.
- Strings are no longer lost or split at chunk boundaries when several encodings
  are extracted together.
- UTF-16LE strings at even and odd offsets that overlap are decided on the whole
  strings, so the result no longer depends on the chunk size, and correct strings
  no longer lose their last characters. The decision is the same however a file
  is split into ranges or a stream into segments.
- Long strings and output waiting for an earlier string are kept in temporary
  files, so memory no longer grows with the input.
- The number of finished chunks waiting to be output is bounded, so parallel
  runs no longer run out of file descriptors.
- An even `--chunk-size` is accepted for an input of odd length.
- A run stopped with Ctrl+C is reported as cancelled, not as a worker failure.
- `-e utf16le-ascii` rejects code units above U+00FF instead of printing their
  low bytes as other characters.
- With both `-e utf16le` and `-e utf16le-ascii`, lines holding only a line break
  are no longer printed, and a line both find at the same offset is printed once.

[2026.11.0]: https://github.com/mnrkbys/pstrings/compare/v0.1.46...v2026.11.0
