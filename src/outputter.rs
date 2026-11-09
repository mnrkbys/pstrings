//! Reassembles chunk fragments per encoding, then emits complete records in
//! global offset order. A completed record waits while an earlier string is
//! still open in another encoding.

use crate::READ_BUFFER_SIZE;
use crate::encoding::InputEncoding;
use crate::record::{MatchRecord, RecordData, RecordDataRef, append_data, read_record_borrowed};
use crate::record_queue::RecordQueue;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

// The output line ending is platform-native, so the same input yields
// byte-different output on Windows and Unix. Tests must compare against
// `LINE_ENDING` (or split on lines) rather than hard-coding "\r\n".
#[cfg(windows)]
const LINE_ENDING: &str = "\r\n";
#[cfg(not(windows))]
const LINE_ENDING: &str = "\n";

/// Per-encoding tally of records actually written to the final output.
///
/// # Why this is counted here rather than in the scanners
///
/// The scanners' own record counts (`DetailedStats::records_by_encoding`)
/// measure scanner work, and legitimately depend on `--chunk-size`: a
/// string crossing N chunk boundaries is emitted as N fragments, and a
/// fragment touching a boundary is emitted even when it is shorter than
/// `--min-length`, since the next chunk might extend it. Both effects
/// inflate the count as chunks shrink, without changing the output at all
/// -- on a 2 KiB test file, UTF-8 reported 744 records at `--chunk-size 2`
/// versus 45 at one chunk, while the output was 44 lines either way.
///
/// The number a user actually wants ("how many strings were found") is
/// therefore only knowable at the point of writing, after fragments have
/// been rejoined and `--min-length` has been applied to the joined
/// result. Counting inside the two write functions means every path --
/// the hot borrowed-record loop, boundary processing, and the final
/// `flush_pending` -- is covered by construction, with no call site able
/// to forget to tally.
///
/// A plain `Mutex<HashMap>` is used rather than atomics per encoding
/// because the entire output stage is single-threaded (one `Receiver`
/// draining chunks in order), so this is uncontended; the cost is a few
/// nanoseconds per output line, against writing that line to disk.
static EMITTED: Mutex<Option<HashMap<InputEncoding, u64>>> = Mutex::new(None);

/// Starts tallying output records. Called once, before the output stage,
/// only when the counts will actually be reported -- when they won't,
/// `EMITTED` stays `None` and the counting in the write functions is a
/// single already-`None` check.
pub fn begin_emitted_counts() {
    *EMITTED.lock().unwrap() = Some(HashMap::new());
}

/// Returns the tally collected since `begin_emitted_counts`, if any.
pub fn take_emitted_counts() -> Option<HashMap<InputEncoding, u64>> {
    EMITTED.lock().unwrap().take()
}

#[inline]
fn note_emitted(encoding: InputEncoding) {
    if let Some(map) = EMITTED.lock().unwrap().as_mut() {
        *map.entry(encoding).or_default() += 1;
    }
}

/// Where output lines go, by encoding.
pub trait Sink {
    fn for_encoding(&mut self, encoding: InputEncoding) -> &mut dyn Write;
    fn flush_all(&mut self) -> io::Result<()>;
}

impl<W: Write> Sink for W {
    fn for_encoding(&mut self, _encoding: InputEncoding) -> &mut dyn Write {
        self
    }

    fn flush_all(&mut self) -> io::Result<()> {
        self.flush()
    }
}

/// One output per encoding, for `-o` with `{encoding}`.
pub struct OutputPerEncoding(pub HashMap<InputEncoding, Box<dyn Write>>);

impl Sink for OutputPerEncoding {
    fn for_encoding(&mut self, encoding: InputEncoding) -> &mut dyn Write {
        self.0.get_mut(&encoding).expect("an output for every selected encoding")
    }

    fn flush_all(&mut self) -> io::Result<()> {
        self.0.values_mut().try_for_each(|output| output.flush())
    }
}

/// Which of the scanned strings are output, and where. The scan reports offsets
/// from the start of what it was given; every field but `base` uses those.
/// UTF-16LE parity is still that of the printed offset (`base` included).
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Added to a string's offset to get the offset that is printed.
    pub base: u64,
    /// Only strings starting in `[start, end)` are output. The bytes around
    /// that range are context for deciding what the strings are.
    pub start: u64,
    pub end: u64,
    /// Strings end here at the latest. Only `scanner::utf16le` is scanned
    /// past it, and its strings are cut here on output.
    pub stop: u64,
    /// Whether `stop` can cut a string, as opposed to being the input's end.
    pub cuts: bool,
    /// Whether `scanner::utf16le` reads past `stop`, so that its strings show
    /// whether they really go on.
    pub utf16le_past_stop: bool,
    /// Whether a candidate starting in `[start, end)` beats an overlapping
    /// one starting elsewhere, whatever their lengths. The other candidate is
    /// never output, so it would take the string with it. A file range does
    /// not want this: there, a neighbouring range outputs that candidate.
    pub owned_wins_overlap: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            base: 0,
            start: 0,
            end: u64::MAX,
            stop: u64::MAX,
            cuts: false,
            utf16le_past_stop: false,
            owned_wins_overlap: false,
        }
    }
}

impl View {
    fn owns(&self, offset: u64) -> bool {
        (self.start..self.end).contains(&offset)
    }

    fn reaches_stop(&self, encoding: InputEncoding, end: u64) -> bool {
        if encoding == InputEncoding::Utf16le && self.utf16le_past_stop {
            return end > self.stop;
        }
        self.cuts && end + encoding.max_char_len() > self.stop
    }

    /// Decides whether `record` is output, cutting it and moving it to its
    /// printed offset if so. Says whether the record reached the stop.
    fn admit(&self, record: &mut MatchRecord, min_cch: u64) -> io::Result<Option<bool>> {
        if !self.owns(record.offset) {
            return Ok(None);
        }
        let end = record.offset + record.cb;
        if end > self.stop {
            let RecordData::Spilled(data) = &mut record.data else {
                unreachable!("only queued UTF-16LE records reach past the stop");
            };
            record.cch = data.cut_utf16_text(self.stop - record.offset)?;
            if record.cch < min_cch {
                return Ok(None);
            }
        }
        record.offset += self.base;
        Ok(Some(self.reaches_stop(record.encoding, end)))
    }
}

/// Merges lines that would repeat one another. `scanner::utf16le` and
/// `scanner::utf16le_ascii` are the two encodings sharing a label, so when
/// both run they can print the same string at the same offset. Output is
/// sorted by offset, so only the current offset's lines are kept.
#[derive(Default)]
struct SeenLines {
    active: bool,
    offset: u64,
    texts: Vec<RecordData>,
}

impl SeenLines {
    fn repeats(
        &mut self,
        offset: u64,
        encoding: InputEncoding,
        text: &RecordData,
        dir: Option<&Path>,
    ) -> io::Result<bool> {
        if !self.active || !matches!(encoding, InputEncoding::Utf16le | InputEncoding::Utf16leAscii) {
            return Ok(false);
        }
        if offset != self.offset {
            self.offset = offset;
            self.texts.clear();
        }
        for seen in &self.texts {
            if seen.same_payload(text)? {
                return Ok(true);
            }
        }
        self.texts.push(text.retained_text(dir)?);
        Ok(false)
    }
}

/// State retained between chunks. In addition to unfinished strings, this
/// retains complete records that must wait for an earlier string to close.
#[derive(Default)]
pub struct OutputState {
    pending: HashMap<(InputEncoding, u8), MatchRecord>,
    parity: [Option<RecordQueue>; 2],
    discard_parity: [bool; 2],
    ready: HashMap<InputEncoding, RecordQueue>,
    temp_dir: Option<PathBuf>,
    view: View,
    cut: u64,
    seen: SeenLines,
}

impl OutputState {
    pub fn new(temp_dir: Option<PathBuf>) -> Self {
        Self {
            temp_dir,
            ..Self::default()
        }
    }

    /// Sets what the following chunks' strings are measured against.
    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    /// Merges lines repeating one another. Needed when the two encodings
    /// sharing a label go to the same output.
    pub fn merge_repeated_lines(&mut self) {
        self.seen.active = true;
    }

    /// How many output strings were stopped by the end of the view.
    pub fn cut_count(&self) -> u64 {
        self.cut
    }

    fn enqueue(&mut self, record: MatchRecord) -> io::Result<()> {
        if record.encoding == InputEncoding::Utf16le {
            // By printed offset, so that a tie in `resolve_parities` goes to
            // the even candidate of the whole input however it was cut.
            // XOR: the sum can overflow for a candidate that is never output.
            let parity = ((self.view.base ^ record.offset) & 1) as usize;
            if self.parity[parity].is_none() {
                self.parity[parity] = Some(RecordQueue::new(self.temp_dir.as_deref())?);
            }
            return self.parity[parity].as_mut().unwrap().push(&record);
        }
        self.enqueue_final(record)
    }

    fn enqueue_final(&mut self, record: MatchRecord) -> io::Result<()> {
        if !self.ready.contains_key(&record.encoding) {
            self.ready
                .insert(record.encoding, RecordQueue::new(self.temp_dir.as_deref())?);
        }
        self.ready.get_mut(&record.encoding).unwrap().push(&record)
    }

    fn complete(&mut self, rec: MatchRecord, min_cch: u64) -> io::Result<()> {
        let leftover = if matches!(&rec.data, RecordData::Spilled(data) if data.raw) {
            let dir = self.temp_dir.clone();
            crate::payload::decode_spilled(rec, dir.as_deref(), |record| {
                if record.cch >= min_cch {
                    self.enqueue(record)?;
                }
                Ok(())
            })?
        } else if matches!(&rec.data, RecordData::Spilled(_)) {
            if rec.cch >= min_cch {
                self.enqueue(rec)?;
            }
            None
        } else {
            let (records, leftover) = resolve_for_output(rec, min_cch);
            for record in records {
                self.enqueue(record)?;
            }
            leftover
        };
        if let Some(record) = leftover {
            self.pending
                .insert(stream_key(record.encoding, record.offset), record);
        }
        Ok(())
    }

    fn parity_bound(&self) -> (u64, u16) {
        self.parity
            .iter()
            .filter_map(|queue| queue.as_ref().and_then(|q| q.front))
            .map(|head| (head.offset, head.encoding as u16))
            .min()
            .unwrap_or((u64::MAX, u16::MAX))
    }

    fn finish_parity(&mut self, parity: usize) -> io::Result<()> {
        let record = self.parity[parity].as_mut().unwrap().pop()?;
        if !std::mem::replace(&mut self.discard_parity[parity], false) {
            self.enqueue_final(record)?;
        } else {
            drop(record);
        }
        self.parity[parity].as_mut().unwrap().reset_if_empty()
    }

    fn resolve_parities(&mut self, known_before: u64) -> io::Result<()> {
        loop {
            let heads = self
                .parity
                .each_ref()
                .map(|queue| queue.as_ref().and_then(|q| q.front));
            match heads {
                [Some(a), Some(b)] => {
                    let a_end = a.offset + a.cb;
                    let b_end = b.offset + b.cb;
                    if a_end <= b.offset {
                        self.finish_parity(0)?;
                    } else if b_end <= a.offset {
                        self.finish_parity(1)?;
                    } else {
                        let (a_owned, b_owned) = (self.view.owns(a.offset), self.view.owns(b.offset));
                        let prefer_a = if self.view.owned_wins_overlap && a_owned != b_owned {
                            a_owned
                        } else {
                            crate::scanner::utf16le::prefer_candidate(
                                a.flags & 8 != 0,
                                a.cb,
                                b.flags & 8 != 0,
                                b.cb,
                            )
                        };
                        self.discard_parity[usize::from(prefer_a)] = true;
                        if a_end <= b_end {
                            self.finish_parity(0)?;
                        }
                        if b_end <= a_end {
                            self.finish_parity(1)?;
                        }
                    }
                }
                [Some(a), None] if a.offset + a.cb <= known_before => self.finish_parity(0)?,
                [None, Some(b)] if b.offset + b.cb <= known_before => self.finish_parity(1)?,
                _ => break,
            }
        }
        Ok(())
    }

    fn ready_key(&self) -> Option<(u64, u16)> {
        self.ready
            .values()
            .filter_map(|queue| queue.front)
            .map(|head| (head.offset, head.encoding as u16))
            .min()
    }

    fn emit_before(
        &mut self,
        bound: (u64, u16),
        min_cch: u64,
        out: &mut impl Sink,
        str_only: bool,
    ) -> io::Result<()> {
        self.resolve_parities(bound.0)?;
        let bound = bound.min(self.parity_bound());
        let mut scratch = Vec::new();
        while let Some(key) = self.ready_key().filter(|key| *key < bound) {
            let encoding = InputEncoding::try_from(key.1)?;
            let queue = self.ready.get_mut(&encoding).unwrap();
            let mut record = queue.pop()?;
            if let Some(reached_stop) = self.view.admit(&mut record, min_cch)?
                && !self.seen.repeats(
                    record.offset,
                    record.encoding,
                    &record.data,
                    self.temp_dir.as_deref(),
                )?
            {
                self.cut += u64::from(reached_stop);
                write_output_record(&mut out.for_encoding(record.encoding), &record, str_only, &mut scratch)?;
            }
            drop(record);
            queue.reset_if_empty()?;
        }
        Ok(())
    }

    fn lower_bound(&self) -> (u64, u16) {
        self.pending
            .values()
            .map(|r| (r.offset, r.encoding as u16))
            .min()
            .unwrap_or((u64::MAX, u16::MAX))
    }
}

fn stream_key(encoding: InputEncoding, offset: u64) -> (InputEncoding, u8) {
    (
        encoding,
        if encoding == InputEncoding::Utf16le {
            (offset % 2) as u8
        } else {
            0
        },
    )
}

/// Consumes one offset-sorted chunk. Boundary continuation and empty chunks
/// are evaluated independently for each encoding, including encodings whose
/// decoder already consumed this chunk through a boundary peek.
pub fn output_merged_chunk(
    file: File,
    chunk_offset: u64,
    chunk_len: u64,
    state: &mut OutputState,
    min_cch: u64,
    out: &mut impl Sink,
    str_only: bool,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "cancelled",
        ));
    }
    let mut reader = BufReader::with_capacity(READ_BUFFER_SIZE, file);
    let mut previous = std::mem::take(&mut state.pending);
    let mut count = 0usize;
    let mut read_scratch = Vec::new();
    let mut write_scratch = Vec::new();
    while let Some(raw) = read_record_borrowed(&mut reader, &mut read_scratch)? {
        count += 1;
        if count % 4096 == 0 && cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "cancelled",
            ));
        }
        let input_bound = (raw.offset, raw.encoding as u16);
        let earlier = previous
            .values()
            .map(|r| (r.offset, r.encoding as u16))
            .min()
            .unwrap_or((u64::MAX, u16::MAX))
            .min(state.lower_bound())
            .min(state.parity_bound());
        if raw.encoding != InputEncoding::Utf16le
            && !raw.ends_at_chunk
            && !previous.contains_key(&stream_key(raw.encoding, raw.offset))
            && input_bound < earlier
        {
            if let RecordDataRef::Text(text) = raw.data {
                state.emit_before(input_bound, min_cch, out, str_only)?;
                if state.ready_key().is_none_or(|key| key >= input_bound) {
                    let offset = state.view.base + raw.offset;
                    if raw.cch >= min_cch
                        && state.view.owns(raw.offset)
                        && !(state.seen.active
                            && state.seen.repeats(
                                offset,
                                raw.encoding,
                                &RecordData::Text(text.to_owned()),
                                state.temp_dir.as_deref(),
                            )?)
                    {
                        let end = raw.offset + raw.cb;
                        state.cut += u64::from(state.view.reaches_stop(raw.encoding, end));
                        write_output_record_str(
                            &mut out.for_encoding(raw.encoding),
                            offset,
                            raw.encoding,
                            text,
                            str_only,
                            &mut write_scratch,
                        )?;
                    }
                    continue;
                }
            }
        }
        let mut rec = owned_from_raw_record(&raw);
        if let Some(prev) = previous.remove(&stream_key(rec.encoding, rec.offset)) {
            if rec.starts_at_chunk && prev.offset + prev.cb == rec.offset {
                let mut merged = prev;
                append_data(&mut merged, rec, state.temp_dir.as_deref())?;
                rec = merged;
            } else {
                state.complete(prev, min_cch)?;
            }
        }
        if rec.ends_at_chunk {
            rec.data.spill(state.temp_dir.as_deref())?;
            state
                .pending
                .insert(stream_key(rec.encoding, rec.offset), rec);
        } else {
            state.complete(rec, min_cch)?;
        }
        let previous_bound = previous
            .values()
            .map(|r| (r.offset, r.encoding as u16))
            .min()
            .unwrap_or((u64::MAX, u16::MAX));
        state.emit_before(
            input_bound.min(previous_bound).min(state.lower_bound()),
            min_cch,
            out,
            str_only,
        )?;
    }
    let chunk_end = chunk_offset.saturating_add(chunk_len);
    for (encoding, prev) in previous {
        if chunk_len > 0 && prev.offset.saturating_add(prev.cb) >= chunk_end {
            state.pending.insert(encoding, prev);
        } else {
            state.complete(prev, min_cch)?;
        }
    }
    state.emit_before(state.lower_bound().min((chunk_end, 0)), min_cch, out, str_only)
}

/// Finishes the final pending strings and drains all complete records in order.
pub fn flush_pending(
    state: &mut OutputState,
    min_cch: u64,
    out: &mut impl Sink,
    str_only: bool,
) -> io::Result<()> {
    for (_, rec) in std::mem::take(&mut state.pending) {
        state.complete(rec, min_cch)?;
    }
    state.pending.clear(); // incomplete final code units have no continuation
    state.emit_before((u64::MAX, u16::MAX), min_cch, out, str_only)
}

#[cfg(test)]
mod seen_lines_tests {
    use super::*;

    #[test]
    fn a_line_repeating_any_earlier_line_at_its_offset_is_found() {
        let mut seen = SeenLines { active: true, ..SeenLines::default() };
        let mut repeats = |offset, text: &str| {
            let text = RecordData::Text(text.into());
            seen.repeats(offset, InputEncoding::Utf16le, &text, None).unwrap()
        };
        assert!(!repeats(7, "A"));
        assert!(!repeats(7, "B"));
        assert!(repeats(7, "A"));
        assert!(repeats(7, "B"));
        assert!(!repeats(8, "A"));
    }

    #[test]
    fn other_encodings_are_left_alone() {
        let mut seen = SeenLines { active: true, ..SeenLines::default() };
        let text = RecordData::Text("A".into());
        assert!(!seen.repeats(7, InputEncoding::Ascii, &text, None).unwrap());
        assert!(!seen.repeats(7, InputEncoding::Ascii, &text, None).unwrap());
    }

    #[test]
    fn long_texts_are_kept_and_compared_on_disk() {
        let long = "A".repeat(crate::payload::MEMORY_LIMIT + 1);
        let mut first = RecordData::Text(long.clone());
        first.spill(None).unwrap();
        let mut other = RecordData::Text(long.clone() + "B");
        other.spill(None).unwrap();
        let mut same = RecordData::Text(long.clone());
        same.spill(None).unwrap();

        let mut seen = SeenLines { active: true, ..SeenLines::default() };
        let encoding = InputEncoding::Utf16le;
        assert!(!seen.repeats(7, encoding, &first, None).unwrap());
        assert!(matches!(seen.texts[0], RecordData::Spilled(_)));
        assert!(!seen.repeats(7, encoding, &other, None).unwrap());
        assert!(seen.repeats(7, encoding, &same, None).unwrap());
    }
}

fn owned_from_raw_record(rec: &crate::record::RawRecord<'_>) -> MatchRecord {
    MatchRecord {
        offset: rec.offset,
        cb: rec.cb,
        cch: rec.cch,
        encoding: rec.encoding,
        starts_at_chunk: rec.starts_at_chunk,
        ends_at_chunk: rec.ends_at_chunk,
        data: match rec.data {
            RecordDataRef::Text(s) => RecordData::Text(s.to_owned()),
            RecordDataRef::Raw(b) => RecordData::Raw(b.to_owned()),
        },
    }
}

fn resolve_for_output(rec: MatchRecord, min_cch: u64) -> (Vec<MatchRecord>, Option<MatchRecord>) {
    // Destructure up front (a full move of `rec`, which is fine since we
    // never need `rec` as a whole again) rather than matching on
    // `rec.data` directly -- matching on just that field would partially
    // move it out of `rec`, making the other fields unusable for
    // reconstructing a record afterward.
    let MatchRecord {
        offset,
        cb,
        cch,
        encoding,
        starts_at_chunk,
        ends_at_chunk,
        data,
    } = rec;

    match data {
        RecordData::Spilled(_) => unreachable!("spilled records resolve through OutputState"),
        RecordData::Text(s) => {
            let keep = cch >= min_cch;
            let written = if keep {
                vec![MatchRecord {
                    offset,
                    cb,
                    cch,
                    encoding,
                    starts_at_chunk,
                    ends_at_chunk,
                    data: RecordData::Text(s),
                }]
            } else {
                vec![]
            };
            (written, None)
        }
        RecordData::Raw(bytes) => {
            let (fragments, tail) = crate::scanner::segment_raw(encoding, &bytes);
            let consumed = bytes.len() - tail.len();
            let written: Vec<MatchRecord> = fragments
                .into_iter()
                .filter(|f| f.cch >= min_cch)
                .map(|f| MatchRecord {
                    offset: offset + f.start,
                    cb: f.cb,
                    cch: f.cch,
                    encoding,
                    // Both boundary flags are now moot: this record has
                    // already been fully resolved (and, if it needed to
                    // keep waiting, that need is represented by the
                    // separate `leftover` return instead).
                    starts_at_chunk: false,
                    ends_at_chunk: false,
                    data: RecordData::Text(f.data),
                })
                .collect();
            let leftover = if tail.is_empty() {
                None
            } else {
                Some(MatchRecord {
                    offset: offset + consumed as u64,
                    cb: tail.len() as u64,
                    cch: 0,
                    encoding,
                    starts_at_chunk,
                    ends_at_chunk: true,
                    data: RecordData::Raw(tail),
                })
            };
            (written, leftover)
        }
    }
}

/// Serializes one owned `MatchRecord` into the final output format.
///
/// In normal mode the format is `<20-digit offset>\\t<encoding>\\t<data><line ending>`.
/// With `str_only`, only the string data and line ending are emitted.
///
/// `rec.data` must be `RecordData::Text` here -- by the time a record
/// reaches this function it has always already been resolved (via
/// `resolve_for_output`, for anything that started out as `Raw`).
pub fn write_output_record(
    out: &mut impl Write,
    rec: &MatchRecord,
    str_only: bool,
    scratch: &mut Vec<u8>,
) -> io::Result<()> {
    let text = match &rec.data {
        RecordData::Spilled(data) if !data.raw => {
            scratch.clear();
            if !str_only {
                let mut number = itoa::Buffer::new();
                let number = number.format(rec.offset);
                scratch.resize(20usize.saturating_sub(number.len()), b'0');
                scratch.extend_from_slice(number.as_bytes());
                scratch.push(b'\t');
                scratch.extend_from_slice(rec.encoding.name().as_bytes());
                scratch.push(b'\t');
            }
            out.write_all(scratch)?;
            data.copy_to(out)?;
            out.write_all(LINE_ENDING.as_bytes())?;
            note_emitted(rec.encoding);
            return Ok(());
        }
        RecordData::Spilled(_) => unreachable!("unresolved raw spilled record"),
        RecordData::Text(s) => s.as_str(),
        RecordData::Raw(_) => {
            debug_assert!(
                false,
                "write_output_record called with an unresolved Raw record for encoding {:?}",
                rec.encoding
            );
            return Ok(());
        }
    };

    // Reuse the caller-provided buffer instead of allocating a temporary String for
    // every output record.
    scratch.clear();
    if str_only {
        scratch.extend_from_slice(text.as_bytes());
        scratch.extend_from_slice(LINE_ENDING.as_bytes());
    } else {
        // `itoa` handles integer formatting without an intermediate heap allocation.
        // The output requires a fixed-width, 20-digit decimal offset, so the missing
        // leading digits are inserted manually.
        let mut num_buf = itoa::Buffer::new();
        let num_str = num_buf.format(rec.offset);
        for _ in 0..(20usize.saturating_sub(num_str.len())) {
            scratch.push(b'0');
        }
        scratch.extend_from_slice(num_str.as_bytes());
        scratch.push(b'\t');
        scratch.extend_from_slice(rec.encoding.name().as_bytes());
        scratch.push(b'\t');
        scratch.extend_from_slice(text.as_bytes());
        scratch.extend_from_slice(LINE_ENDING.as_bytes());
    }
    note_emitted(rec.encoding);
    out.write_all(scratch)
}

/// Equivalent to `write_output_record` for a borrowed string slice. This avoids creating
/// a temporary `MatchRecord` when the record came directly from the intermediate stream.
fn write_output_record_str(
    out: &mut impl Write,
    offset: u64,
    encoding: InputEncoding,
    data: &str,
    str_only: bool,
    scratch: &mut Vec<u8>,
) -> io::Result<()> {
    scratch.clear();
    if str_only {
        scratch.extend_from_slice(data.as_bytes());
        scratch.extend_from_slice(LINE_ENDING.as_bytes());
    } else {
        let mut num_buf = itoa::Buffer::new();
        let num_str = num_buf.format(offset);
        for _ in 0..(20usize.saturating_sub(num_str.len())) {
            scratch.push(b'0');
        }
        scratch.extend_from_slice(num_str.as_bytes());
        scratch.push(b'\t');
        scratch.extend_from_slice(encoding.name().as_bytes());
        scratch.push(b'\t');
        scratch.extend_from_slice(data.as_bytes());
        scratch.extend_from_slice(LINE_ENDING.as_bytes());
    }
    note_emitted(encoding);
    out.write_all(scratch)
}
