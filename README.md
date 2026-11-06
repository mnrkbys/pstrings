# pstrings

Version 0.2.0 is a personal fork maintained by mnrkbys, based on
[upstream pstrings v0.1.46](https://github.com/herosi/pstrings/tree/v0.1.46).
This fork is maintained independently of the upstream project.

## pstrings - Parallel strings extractor for very large files


## Features

- Extracts both ASCII and UNICODE (UTF-16LE) strings at once (like Sysinternals strings)
- Runs fast using multiple threads
- Supports multiple encodings (Experimental)
  - UTF-8, UTF-16LE, CP932 (Shift-JIS), ISO-2022-JP, Windows-1252 (Latin1), EUC-KR, GB18030, GBK, BIG5, Windows-1251
- Uses bounded output buffers and temporary files for long strings and delayed records


## Use Cases

- Digital forensics: extracting readable strings present in memory images or unallocated disk space


## Speed Comparison

### Results

```mermaid
---
config:
    xyChart:
        showDataLabel: true
        showDataLabelOutsideBar: true
---
xychart
title "Time taken by each tool to analyze a 5.5GB file"
x-axis "strings" ["pstrings", "bstrings", "Msys2", "Sysinternals"]
y-axis "seconds" 0 --> 3600
bar [14,145,3303,3860]
```

### Conditions
- CPU
  Intel Core i9 2.80GHz (32 Core)
- Memory
  128 GB
- OS
  Windows Server 2025 Standard
- Input file size
  5.50 GB

### pstrings 0.1.45

```powershell
Measure-Command { .\Tools\pstrings.exe -s -o output.txt xxx.bin }

Days              : 0
Hours             : 0
Minutes           : 0
Seconds           : 13
Milliseconds      : 884
Ticks             : 138844483
TotalDays         : 0.000160699633101852
TotalHours        : 0.00385679119444444
TotalMinutes      : 0.231407471666667
TotalSeconds      : 13.8844483
TotalMilliseconds : 13884.4483
```


### Sysinternals strings v2.54

```powershell
Measure-Command { .\Tools\SysinternalsSuite\strings.exe -nobanner -n 4 xxx.bin > output.txt }

Days              : 0
Hours             : 1
Minutes           : 4
Seconds           : 20
Milliseconds      : 141
Ticks             : 38601414731
TotalDays         : 0.0446775633460648
TotalHours        : 1.07226152030556
TotalMinutes      : 64.3356912183333
TotalSeconds      : 3860.1414731
TotalMilliseconds : 3860141.4731
```


### MSys2 bintuils strings 2.46

```powershell 
Measure-Command { `
  .\Tools\msys64\usr\bin\strings.exe -e s xxx.bin > output.txt ; `
  .\Tools\msys64\usr\bin\strings.exe -e l xxx.bin >> output.txt ; `
}

Days              : 0
Hours             : 0
Minutes           : 55
Seconds           : 3
Milliseconds      : 367
Ticks             : 33033679276
TotalDays         : 0.038233425087963
TotalHours        : 0.917602202111111
TotalMinutes      : 55.0561321266667
TotalSeconds      : 3303.3679276
TotalMilliseconds : 3303367.9276
```


### Eric Zimmerman's bstrings 2026.5.0

```powershell
Measure-Command { .\Tools\EZTools\net9\bstrings.exe -s -m 4 -f xxx.bin -o output.txt }

Days              : 0
Hours             : 0
Minutes           : 2
Seconds           : 25
Milliseconds      : 266
Ticks             : 1452668352
TotalDays         : 0.00168132911111111
TotalHours        : 0.0403518986666667
TotalMinutes      : 2.42111392
TotalSeconds      : 145.2668352
TotalMilliseconds : 145266.8352
```


## Usage

```bash
pstrings input.bin
pstrings -s input.bin
pstrings -s -o output.txt input.bin
pstrings -m 3 -o output.txt input.bin
pstrings -o output.txt input.bin -f ascii,latin1
pstrings -o output.txt input.bin -e utf16le -f kanji-jis1,cjkpunct,hiragana,katakana
pstrings -o output.txt input.bin -e cp932
pstrings -j 8 -o output.txt input.bin
pstrings -o output.txt --temp-dir D:\PathToSsdOrFastDrive input.bin
pstrings -o "output-{encoding}.tsv" -e ascii,utf16le input.bin
pstrings --offset 0x100000 --length 64M input.bin
read_image | pstrings -
read_regions | pstrings --segments regions.tsv -
```

### Extracting part of an input

`--offset` and `--length` give a range of a file. A range outputs the strings of
the whole file that start in it, at their file offsets, so extracting consecutive
ranges gives the same lines as extracting the file at once, for strings no longer
than `--extend-limit` bytes. A longer string is cut by the range it starts in, and
the rest of it is not output. To find what belongs to the range, up to
`max(--extend-limit, 64 KiB)` bytes before it are read for every encoding, and
UTF-16LE (`-e utf16le`) is also read that far past the limit: which of two
overlapping candidates is kept depends on their full lengths. CP932-like encodings
and ISO-2022-JP find character boundaries from a byte that cannot be part of a
string, which has to lie in the bytes read before the range for the result to
match the whole file.

`--segments` describes standard input as pieces from unrelated places, such as the
unallocated parts of a volume. Each piece is extracted on its own: nothing precedes
it, and no string continues into the next one. The input is one line per piece,
`<base_offset>\t<owned_length>\t<extension_length>` in decimal.

In a segment a string starting in the owned part is kept over an overlapping UTF-16LE
candidate starting in the extension, as that candidate is not output. A file range
decides as the whole file does, since the next range outputs the candidate.

`cut_strings` in `--stats` counts output strings that reached the limit (within one
character of it). A string ending just before the limit by itself may be counted
too, so the count can be too high. `-e utf16le` with a file range is exact instead,
as it reads past the limit. When the end of the file is reached first, nothing is
counted, and neither is a segment with an empty extension. What remains of a cut
string is subject to `--min-length` as usual.

`-e utf16le` and `-e utf16le-ascii` print the same label, so a line both would
print is printed once when they share an output. With `{encoding}` in `-o` each
gets its own file, which holds what that encoding gives on its own.


## Arguments/Options

```bash
> pstrings --help

Parallel strings extractor for very large files

Usage: pstrings [OPTIONS] <INPUT>

Arguments:
  <INPUT>
          Input file, or - to read a stream from standard input. A stream is read once, in order, and never written to disk; the output is the same as for a file holding the same bytes

Options:
  -s, --string-only
          Omit the offset and encoding columns, printing only the matched text

  -o, --output <OUTPUT>
          Output file. [default: stdout]. A path containing {encoding} is a
          template: each encoding goes to its own file, named by replacing
          {encoding} with its -e name, as if extracted on its own.

  -e, --encoding <ENCODING>
          Input encoding(s), repeated or comma-separated. [default: ascii, utf16le-ascii]
          
          [possible values: ascii, utf16le-ascii, utf16le, utf8, iso2022-jp, cp932, gbk, euc-kr, windows-1251, big5, gb18030]

  -f, --filter <FILTER>
          Character filter(s): which characters may appear in a match.
          Repeat the option or comma-separate to select multiple; a
          character is kept if any selected filter allows it.
          [default: ascii].
          
          A "string" in a binary file is a guess, and most encodings can
          check the guess themselves: UTF-8 has strict well-formedness
          rules, and the CJK multi-byte encodings only accept sequences
          their standard assigns. Two cannot. In UTF-16LE any byte pair is a
          valid code unit, and in windows-1251 every byte is a character, so
          scanning either without restricting *which* characters count would
          report most of the file as text. Narrowing pays off sharply: if a
          fraction p of characters are admitted, false positives scale as
          p^min-length.
          
            utf16le, windows-1251   essential, as above
            ascii, utf16le-ascii    only picks ascii vs. ascii,latin1
            all others              ignored (they validate structurally)
          
          So dropping ascii to quiet utf16le will not silently narrow utf8,
          cp932, gbk, gb18030, euc-kr, big5 or iso2022-jp -- they always
          match plain ASCII regardless.
          
          Only ascii and latin1 have a single-byte form and cyrillic is also
          wired into windows-1251; every other filter is useful only with
          -e utf16le. The three kanji filters are nested, narrowest first:
          kanji-jis1, kanji-jis2, kanji.
          
          printable goes the other way: it admits everything except
          controls, surrogates and private use, for pulling out all the text
          and narrowing it down afterwards. It admits 87% of the BMP, so
          expect roughly half of any random binary region to match at the
          default -m 4 -- raise -m well above that when using it.
          
          EXAMPLES
          
            Japanese in a UTF-16LE binary:
              -e utf16le -f kanji-jis1,hiragana,katakana,cjkpunct
          
            Russian text in a windows-1251 file:
              -e windows-1251 -f ascii,cyrillic
          
            Western European text, single-byte:
              -e ascii -f ascii,latin1
          
            Everything, to filter yourself later:
              -e utf16le -f printable -m 12

          Possible values:
          - ascii:        96 points. Printable ASCII plus tab
          - latin1:       96 points. U+00A0-U+00FF, accented Latin and symbols
          - cyrillic:     256 points. U+0400-U+04FF, Russian and its neighbours
          - kanji:        27,584 points. Han U+4E00-U+9FFF + Ext A U+3400-U+4DBF
          - kanji-jis1:   2,965 points. JIS X 0208 level 1, all 2,136 joyo kanji
          - kanji-jis2:   6,355 points. JIS X 0208 lv 1+2, adds jinmeiyo kanji
          - kanji-ext-b:  42,720 points. Han Ext B, U+20000-U+2A6DF
          - hiragana:     91 points. U+3041-U+3096 and U+309B-U+309F
          - katakana:     96 points. U+30A0-U+30FF, fullwidth only
          - hangul:       11,585 points. Syllables U+AC00-U+D7A3 + 3 jamo blocks
          - cjkpunct:     196 points. CJK punct, fullwidth ASCII, halfwidth kana
          - cjkpunct-all: 304 points. All of U+3000-U+303F and U+FF00-U+FFEF
          - printable:    253,592 points. All but controls, surrogates, private

  -m, --min-length <MIN_LENGTH>
          Minimum number of decoded characters (cch). Must be at least 1
          
          [default: 4]

  -j, --jobs <JOBS>
          Number of worker threads. [default: ncpus]

  -c, --chunk-size <CHUNK_SIZE>
          Chunk size: K/M/G/T suffixes, or "auto"
          
          [default: auto]

      --offset <OFFSET>
          Start of the file range to extract, as a byte offset. Only strings
          starting in the range are output, and their offsets stay relative to
          the start of the file. A string that started before the range belongs
          to the range before, so it is not output, even in part. To tell, up
          to max(--extend-limit, 64 KiB) bytes before the offset are read, for
          every encoding; CP932-like encodings need a byte that cannot be part
          of a character within those bytes to find character boundaries.
          Accepts decimal, 0x hexadecimal, and K/M/G/T suffixes. Must be before
          the end of the file. Not available with -.

      --length <LENGTH>
          Length of the file range in bytes: how much of the file the range
          owns, not how much is read. A string starting in the range is read
          past its end, up to --extend-limit. [default: to the end of the file]
          Accepts the same forms as --offset. Must be > 0. A range running past
          the end of the file stops there.

      --extend-limit <EXTEND_LIMIT>
          How far past the end of a --length range a string may run. A longer
          string is cut there, and --stats counts it; the rest of it is not
          output by any range. What remains of a cut string is subject to
          --min-length as usual, so it may not be output at all. Accepts the
          same forms as --offset. [default: 64K]

      --segments <SEGMENTS>
          With -: declares that standard input is a series of segments, each
          given by one line of this file, in order:
          
            <base_offset>\t<owned_length>\t<extension_length>     (decimal)
          
          Standard input must hold exactly owned_length + extension_length
          bytes per segment, back to back. A string is output if it starts in
          the owned part, at offset base_offset + its position in the segment.
          It may run into the extension, and is cut at the segment's end, which
          --stats counts unless the extension is empty. What remains of a cut
          string is subject to --min-length as usual. No string crosses into
          the next segment. A string starting in the owned part is kept over an
          overlapping UTF-16LE candidate starting in the extension. Owned parts
          must be in ascending order and must not overlap.

      --keep-temp
          Keep intermediate chunk result files for debugging

      --temp-dir <TEMP_DIR>
          Directory in which the temporary result directory is created

      --stats
          Print processing statistics, including peak RSS. Same as -v.
          
          cut_strings counts output strings that stopped at --extend-limit or
          at the end of a segment's extension. A string ending just before the
          limit by itself may be counted too, so this can be too high. Segments
          with an empty extension are not counted.

  -v, --verbose...
          Print processing statistics (as --stats). Repeat (-vv) to add a per-phase breakdown: scan time and record counts per encoding, plus merge and output timings

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```


## How to Build

```bash
cargo build --release
```

I tested on Windows 11, Server 2025 and Ubuntu 24.04 on x64.

## Temporary storage and memory

Worker memory depends on the selected scanners and chunk size. The full UTF-16LE
scanner retains candidates for a chunk, so memory use is not a fixed amount per
worker. Strings crossing chunk boundaries spill to temporary files when their
payload exceeds 8 MiB. Completed records waiting behind an earlier string are
also queued on disk so output stays sorted by offset.

Spilling preserves the complete strings; it does not impose a maximum string
length. It trades memory for temporary disk space and I/O. A long run may need
space for its source bytes, decoded text, and queued output while it is being
resolved. Use `--temp-dir` to select a filesystem with enough free space. These
spool files are removed when their records or output state are dropped.

Standard input is held in memory one chunk per worker, and a chunk is released when
its scan completes. Its bytes are never written to disk, though the strings found
in it are, as for any input. `--chunk-size auto` means 16 MiB for standard input.
