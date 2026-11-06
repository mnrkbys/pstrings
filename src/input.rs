//! Where scanned bytes come from: hands workers one chunk at a time, either
//! as a range of a file or as an in-memory buffer read from a stream.

use crate::chunk::{Chunk, STREAM_CHUNK_SIZE};
use crate::scanner::ByteSource;
use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Bytes kept on each side of a streamed chunk. Scanners look a few bytes past
/// their chunk (`scanner::utf16le`'s surrogate checks, `scanner::utf8`'s peek),
/// at most 3 in either direction; a stream cannot go back for them.
const CONTEXT: u64 = 4;

/// One unit of scanning. `input_len` is what the scanners take as the input's
/// length: it only has to say how many bytes follow the chunk, up to `CONTEXT`.
pub struct Work {
    pub index: u64,
    pub source: Arc<dyn ByteSource>,
    pub chunk: Chunk,
    pub input_len: u64,
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
        Ok(Some(Work { index, source: Arc::new(source), chunk, input_len }))
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
    pub fn file(source: Arc<dyn ByteSource>, len: u64, chunk_size: u64) -> Self {
        Feed {
            kind: Kind::File(FileFeed {
                source,
                len,
                chunk_size,
                next_index: AtomicU64::new(0),
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
