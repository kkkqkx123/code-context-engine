//! Bounded-memory external sort for canonical snapshot records.
//!
//! Records are buffered as `(sort key, payload)` byte pairs. Once a chunk
//! reaches its byte budget it is sorted and spilled to a temporary file; the
//! final iterator performs a stable k-way merge, so callers never hold the
//! whole project in memory.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use std::io::Result as IoResult;

const RECORD_HEADER_LEN: usize = 8;

struct Record {
    key: Vec<u8>,
    payload: Vec<u8>,
    sequence: u64,
}

impl Record {
    fn write_to(&self, writer: &mut impl Write) -> IoResult<()> {
        let key_len = u32::try_from(self.key.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "external sort key exceeds u32",
            )
        })?;
        let payload_len = u32::try_from(self.payload.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "external sort payload exceeds u32",
            )
        })?;
        writer.write_all(&key_len.to_le_bytes())?;
        writer.write_all(&payload_len.to_le_bytes())?;
        writer.write_all(&self.key)?;
        writer.write_all(&self.payload)?;
        Ok(())
    }

    fn read_from(reader: &mut impl Read) -> IoResult<Option<Self>> {
        let mut header = [0u8; RECORD_HEADER_LEN];
        match reader.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(error),
        }
        let key_len =
            u32::from_le_bytes(header[..4].try_into().expect("four key length bytes")) as usize;
        let payload_len =
            u32::from_le_bytes(header[4..].try_into().expect("four payload length bytes")) as usize;
        let mut key = vec![0u8; key_len];
        reader.read_exact(&mut key)?;
        let mut payload = vec![0u8; payload_len];
        reader.read_exact(&mut payload)?;
        Ok(Some(Self {
            key,
            payload,
            sequence: 0,
        }))
    }
}

/// Sort records in bounded chunks and merge them on iteration.
pub struct ExternalSorter {
    directory: PathBuf,
    name: String,
    chunk_bytes: usize,
    buffer: Vec<Record>,
    buffered_bytes: usize,
    chunks: Vec<PathBuf>,
    sequence: u64,
}

impl ExternalSorter {
    pub fn new(directory: &Path, name: &str, chunk_bytes: usize) -> Self {
        Self {
            directory: directory.to_path_buf(),
            name: name.to_string(),
            chunk_bytes: chunk_bytes.max(1),
            buffer: Vec::new(),
            buffered_bytes: 0,
            chunks: Vec::new(),
            sequence: 0,
        }
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffered_bytes
    }

    pub fn push(&mut self, key: Vec<u8>, payload: Vec<u8>) -> IoResult<()> {
        let bytes = key.len() + payload.len() + RECORD_HEADER_LEN;
        if !self.buffer.is_empty() && self.buffered_bytes + bytes > self.chunk_bytes {
            self.flush_chunk()?;
        }
        self.buffer.push(Record {
            key,
            payload,
            sequence: self.sequence,
        });
        self.sequence += 1;
        self.buffered_bytes += bytes;
        Ok(())
    }

    fn flush_chunk(&mut self) -> IoResult<()> {
        self.buffer
            .sort_by(|a, b| a.key.cmp(&b.key).then(a.sequence.cmp(&b.sequence)));
        let path = self
            .directory
            .join(format!("{}-{}.sort", self.name, self.chunks.len()));
        let mut writer = BufWriter::new(File::create(&path)?);
        for record in &self.buffer {
            record.write_to(&mut writer)?;
        }
        writer.flush()?;
        self.chunks.push(path);
        self.buffer.clear();
        self.buffered_bytes = 0;
        Ok(())
    }

    /// Finish sorting. Any buffered chunk is spilled first so iteration is
    /// identical regardless of how the records were split.
    pub fn into_sorted(mut self) -> IoResult<SortedRecords> {
        if !self.buffer.is_empty() {
            self.flush_chunk()?;
        }
        let readers = self
            .chunks
            .iter()
            .map(|path| Ok(BufReader::new(File::open(path)?)))
            .collect::<IoResult<Vec<_>>>()?;
        Ok(SortedRecords {
            readers,
            heap: BinaryHeap::new(),
            started: false,
        })
    }
}

#[derive(Eq, PartialEq)]
struct MergeEntry {
    key: Vec<u8>,
    payload: Vec<u8>,
    sequence: u64,
    reader: usize,
}

impl Ord for MergeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key
            .cmp(&other.key)
            .then(self.sequence.cmp(&other.sequence))
            .then(self.reader.cmp(&other.reader))
    }
}

impl PartialOrd for MergeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Stable, ascending iterator over spilled records.
pub struct SortedRecords {
    readers: Vec<BufReader<File>>,
    heap: BinaryHeap<Reverse<MergeEntry>>,
    started: bool,
}

impl SortedRecords {
    fn fill(&mut self, reader: usize) -> IoResult<()> {
        if let Some(record) = Record::read_from(&mut self.readers[reader])? {
            self.heap.push(Reverse(MergeEntry {
                key: record.key,
                payload: record.payload,
                sequence: record.sequence,
                reader,
            }));
        }
        Ok(())
    }
}

impl Iterator for SortedRecords {
    type Item = IoResult<(Vec<u8>, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.started {
            self.started = true;
            for index in 0..self.readers.len() {
                if let Err(error) = self.fill(index) {
                    return Some(Err(error));
                }
            }
        }
        let entry = self.heap.pop()?;
        let entry = entry.0;
        if let Err(error) = self.fill(entry.reader) {
            return Some(Err(error));
        }
        Some(Ok((entry.key, entry.payload)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_chunks_in_stable_key_order() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let mut sorter = ExternalSorter::new(directory.path(), "test", 24);
        for (key, payload) in [("b", "2"), ("a", "1"), ("b", "3"), ("a", "4")] {
            sorter
                .push(key.as_bytes().to_vec(), payload.as_bytes().to_vec())
                .expect("push should succeed");
        }
        let sorted: Vec<(String, String)> = sorter
            .into_sorted()
            .expect("sort should succeed")
            .map(|record| {
                let (key, payload) = record.expect("record should read");
                (
                    String::from_utf8(key).expect("key is utf8"),
                    String::from_utf8(payload).expect("payload is utf8"),
                )
            })
            .collect();
        assert_eq!(
            sorted,
            vec![
                ("a".to_string(), "1".to_string()),
                ("a".to_string(), "4".to_string()),
                ("b".to_string(), "2".to_string()),
                ("b".to_string(), "3".to_string()),
            ]
        );
    }
}
