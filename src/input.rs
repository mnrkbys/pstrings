//! Where scanned bytes come from: hands workers one chunk at a time, either
//! as a range of a file or as an in-memory buffer read from a stream.

use crate::chunk::{Chunk, STREAM_CHUNK_SIZE};
use crate::outputter::View;
use crate::scanner::ByteSource;
use std::fs::File;
use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Bytes kept on each side of a streamed chunk. Scanners look a few bytes past
/// their chunk (`scanner::utf16le`'s surrogate checks, `scanner::utf8`'s peek),
/// at most 3 in either direction; a stream cannot go back for them.
const CONTEXT: u64 = 4;

/// `--extend-limit`'s default.
pub const DEFAULT_EXTEND_LIMIT: u64 = 64 * 1024;

/// The least a range looks at around itself. See `plan_file_range`.
const MIN_RANGE_CONTEXT: u64 = 64 * 1024;

/// The part of a file to scan for `--offset`/`--length`.
#[derive(Debug)]
pub struct FileRange {
    pub start: u64,
    pub len: u64,
    pub view: View,
}

/// Works out what to read so that a range outputs exactly the strings that
/// scanning the whole file finds starting in it.
///
/// Reading from before `offset` is what tells a string continuing into the
/// range from one starting there, so a continuation shows up with an offset
/// outside the view and is dropped. One byte would do for ASCII, but the
/// encodings without self-synchronization (CP932, ...) only find character
/// boundaries by reading forward from a byte that cannot be part of a
/// character, and that byte has to be inside the context.
///
/// `scanner::utf16le` needs more. Which of two overlapping candidates of
/// opposite parity survives depends on their lengths (see
/// `OutputState::resolve_parities`), so judging a string needs the whole of
/// every candidate overlapping it. Any candidate within `extend_limit` is
/// fully seen when the context reaches `extend_limit` back, and past the
/// furthest such string's end forward, which is why that one encoding is
/// scanned beyond `stop`; a longer candidate seen in part is still longer
/// than the strings it is compared with, so it decides the same way.
///
/// `start` stays even: both parities are scanned, but a tie prefers the even
/// one, and that has to mean the same thing as in the whole file.
pub fn plan_file_range(
    file_len: u64,
    offset: Option<u64>,
    length: Option<u64>,
    extend_limit: u64,
    utf16le: bool,
) -> Result<FileRange, String> {
    if offset.is_none() && length.is_none() {
        return Ok(FileRange { start: 0, len: file_len, view: View::default() });
    }
    let offset = offset.unwrap_or(0);
    if offset >= file_len {
        return Err(format!("--offset {offset} is not before the end of the file ({file_len} bytes)"));
    }
    if length == Some(0) {
        return Err("--length must be > 0".into());
    }
    let end = length.map_or(file_len, |len| offset.saturating_add(len).min(file_len));
    let stop = end.saturating_add(extend_limit).min(file_len);
    let context = extend_limit.max(MIN_RANGE_CONTEXT);
    let start = offset.saturating_sub(context) & !1;
    let scan_end = if utf16le { stop.saturating_add(context).min(file_len) } else { stop };
    Ok(FileRange {
        start,
        len: scan_end - start,
        view: View {
            base: start,
            start: offset - start,
            end: end - start,
            stop: stop - start,
            cuts: stop < file_len,
        },
    })
}

/// A file seen from `start`, so that the start of a range is the start of the
/// input to the scanners.
pub struct FileWindow {
    pub file: Arc<File>,
    pub start: u64,
}

impl ByteSource for FileWindow {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        self.file.as_ref().read_at(buf, self.start + offset)
    }
}

/// One unit of scanning. `input_len` is what the scanners take as the input's
/// length: it only has to say how many bytes follow the chunk, up to `CONTEXT`.
pub struct Work {
    pub index: u64,
    pub source: Arc<dyn ByteSource>,
    pub chunk: Chunk,
    pub input_len: u64,
    pub view: View,
}

/// A slice of a stream. Offsets are those of the stream as a whole.
struct StreamBuffer {
    start: u64,
    bytes: Vec<u8>,
}

impl ByteSource for StreamBuffer {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        let from = offset
            .checked_sub(self.start)
            .filter(|from| *from <= self.bytes.len() as u64)
            .ok_or_else(|| io::Error::other("read outside the buffered stream"))?
            as usize;
        let len = buf.len().min(self.bytes.len() - from);
        buf[..len].copy_from_slice(&self.bytes[from..from + len]);
        Ok(len)
    }
}

struct FileFeed {
    source: Arc<dyn ByteSource>,
    len: u64,
    chunk_size: u64,
    next_index: AtomicU64,
    view: View,
}

impl FileFeed {
    fn next(&self) -> Option<Work> {
        let index = self.next_index.fetch_add(1, Ordering::Relaxed);
        let offset = index.checked_mul(self.chunk_size).filter(|o| *o < self.len)?;
        Some(Work {
            index,
            source: Arc::clone(&self.source),
            chunk: Chunk::new(offset, self.chunk_size.min(self.len - offset)),
            input_len: self.len,
            view: self.view,
        })
    }
}

/// Reads a stream strictly in order, so each worker holds one chunk at most
/// and nothing is ever written out. The bytes around a chunk's edges are
/// copied into the neighbouring buffer, which is what a file's positioned
/// reads would have found there.
struct StreamFeed {
    reader: Box<dyn Read + Send>,
    chunk_size: u64,
    next_index: u64,
    /// Offset of the next chunk.
    pos: u64,
    /// The bytes read so far that a later chunk still needs.
    buffer: StreamBuffer,
}

impl StreamFeed {
    fn buffer_end(&self) -> u64 {
        self.buffer.start + self.buffer.bytes.len() as u64
    }

    /// Reads until the buffer reaches `end`, or the stream ends.
    fn fill_to(&mut self, end: u64) -> io::Result<()> {
        let want = end.saturating_sub(self.buffer_end());
        self.buffer.bytes.reserve_exact(want as usize);
        (&mut self.reader).take(want).read_to_end(&mut self.buffer.bytes)?;
        Ok(())
    }

    fn next(&mut self) -> io::Result<Option<Work>> {
        self.fill_to(self.pos + self.chunk_size + CONTEXT)?;
        let input_len = self.buffer_end();
        let len = self.chunk_size.min(input_len - self.pos);
        if len == 0 {
            if self.pos == 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "empty input"));
            }
            return Ok(None);
        }

        let chunk = Chunk::new(self.pos, len);
        self.pos += len;
        let keep_from = self.pos.saturating_sub(CONTEXT).max(self.buffer.start);
        let tail = self.buffer.bytes[(keep_from - self.buffer.start) as usize..].to_vec();
        let bytes = std::mem::replace(&mut self.buffer.bytes, tail);
        let source = StreamBuffer { start: self.buffer.start, bytes };
        self.buffer.start = keep_from;

        let index = self.next_index;
        self.next_index += 1;
        Ok(Some(Work {
            index,
            source: Arc::new(source),
            chunk,
            input_len,
            view: View::default(),
        }))
    }
}

enum Kind {
    File(FileFeed),
    Stream(Mutex<StreamFeed>),
}

/// The chunks of one run, shared by every worker.
pub struct Feed {
    kind: Kind,
    /// How many chunks there are, or `u64::MAX` until a stream has ended.
    chunk_total: AtomicU64,
    input_bytes: AtomicU64,
}

impl Feed {
    pub fn file(source: Arc<dyn ByteSource>, len: u64, chunk_size: u64, view: View) -> Self {
        Feed {
            kind: Kind::File(FileFeed {
                source,
                len,
                chunk_size,
                next_index: AtomicU64::new(0),
                view,
            }),
            chunk_total: AtomicU64::new(len.div_ceil(chunk_size)),
            input_bytes: AtomicU64::new(len),
        }
    }

    pub fn stream(reader: Box<dyn Read + Send>, chunk_size: u64) -> Self {
        Feed {
            kind: Kind::Stream(Mutex::new(StreamFeed {
                reader,
                chunk_size,
                next_index: 0,
                pos: 0,
                buffer: StreamBuffer { start: 0, bytes: Vec::new() },
            })),
            chunk_total: AtomicU64::new(u64::MAX),
            input_bytes: AtomicU64::new(0),
        }
    }

    /// The chunk size `--chunk-size auto` means for a stream, whose length
    /// is unknown. Workers each hold about this much input.
    pub fn stream_chunk_size() -> u64 {
        STREAM_CHUNK_SIZE
    }

    pub fn next(&self) -> io::Result<Option<Work>> {
        match &self.kind {
            Kind::File(feed) => Ok(feed.next()),
            Kind::Stream(feed) => {
                let mut feed = feed.lock().unwrap();
                let work = feed.next()?;
                match &work {
                    Some(work) => {
                        self.input_bytes.fetch_add(work.chunk.len(), Ordering::Relaxed);
                    }
                    None => self.chunk_total.store(feed.next_index, Ordering::SeqCst),
                }
                Ok(work)
            }
        }
    }

    pub fn chunk_total(&self) -> &AtomicU64 {
        &self.chunk_total
    }

    pub fn input_bytes(&self) -> u64 {
        self.input_bytes.load(Ordering::Relaxed)
    }
}
