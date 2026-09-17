// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Subfile`: a `Read + Seek` (and `symphonia::io::MediaSource`) adapter
//! that exposes one Spotify audio file's byte range — skipping its 0xa7
//! Ogg-container header offset — for direct `symphonia` decoding
//! (005-now-playing-waveform, research R3, contracts/
//! connect-source-delta.md §1). Structurally identical to librespot-
//! playback's own private `Subfile` (`player.rs`), reimplemented here
//! because that one is not exported.

use std::io::{self, Read, Seek, SeekFrom};

use symphonia::core::io::MediaSource;

pub struct Subfile<T: Read + Seek> {
    stream: T,
    offset: u64,
    length: u64,
}

impl<T: Read + Seek> Subfile<T> {
    /// Seeks `stream` to `offset` immediately, so every subsequent
    /// `Subfile` read/seek is relative to it. `length` is the subfile's
    /// own byte length (`offset..length` of the underlying stream).
    pub fn new(mut stream: T, offset: u64, length: u64) -> Result<Self, io::Error> {
        stream.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            stream,
            offset,
            length,
        })
    }
}

impl<T: Read + Seek> Read for Subfile<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl<T: Read + Seek> Seek for Subfile<T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let pos = match pos {
            SeekFrom::Start(offset) => SeekFrom::Start(offset + self.offset),
            SeekFrom::End(offset) => {
                if (self.length as i64 - offset) < self.offset as i64 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "newpos would be < self.offset",
                    ));
                }
                pos
            }
            SeekFrom::Current(_) => pos,
        };
        let newpos = self.stream.seek(pos)?;
        Ok(newpos - self.offset)
    }
}

impl<R> MediaSource for Subfile<R>
where
    R: Read + Seek + Send + Sync,
{
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_start_at_the_offset() {
        let data = b"HEADERrest-of-the-file".to_vec();
        let mut subfile = Subfile::new(Cursor::new(data), 6, 23).unwrap_or_else(|e| panic!("{e}"));
        let mut out = [0u8; 4];
        subfile
            .read_exact(&mut out)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&out, b"rest");
    }

    #[test]
    fn seek_from_start_is_relative_to_the_offset() {
        let data = b"HEADERrest-of-the-file".to_vec();
        let mut subfile = Subfile::new(Cursor::new(data), 6, 23).unwrap_or_else(|e| panic!("{e}"));
        let pos = subfile
            .seek(SeekFrom::Start(5))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(pos, 5);
        let mut out = [0u8; 3];
        subfile
            .read_exact(&mut out)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&out, b"of-");
    }

    #[test]
    fn byte_len_is_the_subfile_length_not_the_underlying_one() {
        let data = vec![0u8; 100];
        let subfile = Subfile::new(Cursor::new(data), 10, 40).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(subfile.byte_len(), Some(40));
        assert!(subfile.is_seekable());
    }
}
