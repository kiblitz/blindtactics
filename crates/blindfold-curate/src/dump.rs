//! Reading the `.csv.zst` dump.
//!
//! # Why this is not one line of ruzstd
//!
//! `ruzstd::StreamingDecoder` decodes exactly **one** zstd frame and, by its own
//! documentation, expects the stream to contain only that. A `.zst` archive may hold
//! any number of frames, plus *skippable* frames carrying metadata.
//!
//! The Lichess dump is both: byte 0 begins a 12-byte skippable frame, and the real
//! data starts at byte 12. Handed the file directly, `StreamingDecoder::new` fails
//! immediately with `SkipFrame { magic_number: 0x184D2A50 }`.
//!
//! The tempting fix is to seek 12 bytes in and carry on. Don't. If the archive ever
//! held a second data frame — a perfectly legal thing that `zstd -T0` and
//! `--adapt` both produce — a single-frame decoder would stop at the end of the
//! first and report EOF, and the loop upstream would happily curate a **silently
//! truncated dump**. It would look like a smaller puzzle set, not like a bug.
//! (A truncated download of this very file already got past a `curl` exit code of 0
//! today, so this is not a hypothetical failure mode.)
//!
//! So this walks frames properly: skip the skippable ones, decode each data frame in
//! turn, and stop only at real end-of-file.

use std::io::Read as _;

/// Zstd's skippable-frame magic numbers. The low nibble is free for the writer to
/// use, so the whole range means "skippable".
const SKIPPABLE: std::ops::RangeInclusive<u32> = 0x184D_2A50..=0x184D_2A5F;

/// Bytes in a frame magic number, and in a skippable frame's length field.
const MAGIC_BYTES: usize = 4;

type Source = std::io::BufReader<std::fs::File>;
type Decoder = ruzstd::StreamingDecoder<Source, ruzstd::FrameDecoder>;

/// A reader over every data frame of a zstd archive, in order.
pub struct Archive {
    /// `None` once the last frame is exhausted.
    current: Option<Decoder>,
}

impl Archive {
    pub fn open(path: &str) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let source = std::io::BufReader::new(file);
        Ok(Self {
            current: next_frame(source)?,
        })
    }
}

/// Advance to the next data frame, skipping any skippable frames on the way.
///
/// `Ok(None)` means genuine end of file.
fn next_frame(mut source: Source) -> std::io::Result<Option<Decoder>> {
    loop {
        let mut magic = [0u8; MAGIC_BYTES];
        match source.read_exact(&mut magic) {
            Ok(()) => {}
            // The only clean place to stop: a frame boundary that is also EOF.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }

        if !SKIPPABLE.contains(&u32::from_le_bytes(magic)) {
            // A data frame. Put the magic back — StreamingDecoder reads the header
            // itself — and hand the stream over.
            source.seek_relative(-(MAGIC_BYTES as i64))?;
            return Ok(Some(ruzstd::StreamingDecoder::new(source).map_err(
                |e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            )?));
        }

        let mut length = [0u8; MAGIC_BYTES];
        source.read_exact(&mut length)?;
        source.seek_relative(i64::from(u32::from_le_bytes(length)))?;
    }
}

impl std::io::Read for Archive {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let Some(decoder) = self.current.as_mut() else {
                return Ok(0);
            };
            let n = decoder.read(buf)?;
            if n > 0 {
                return Ok(n);
            }
            // This frame is spent. `read` returning 0 is what a single-frame decoder
            // calls EOF; for us it only means "next frame", and the loop is what
            // stops a multi-frame archive from being read as its first frame alone.
            let spent = self.current.take().expect("checked just above");
            self.current = next_frame(spent.into_inner())?;
            if self.current.is_none() {
                return Ok(0);
            }
        }
    }
}

/// These live here rather than in `tests/` because this is a binary crate: an
/// integration test cannot import a bin target's modules.
#[cfg(test)]
mod tests {
    use super::*;

    /// A zstd data frame holding `content` as a single uncompressed block.
    ///
    /// Hand-built rather than compressed with a library, because the point is to
    /// control the *frame layout* — how many frames and in what order — which a
    /// compressor picks for you.
    fn data_frame(content: &[u8]) -> Vec<u8> {
        assert!(
            content.len() <= u8::MAX as usize,
            "1-byte content size field"
        );
        let mut out = vec![0x28, 0xB5, 0x2F, 0xFD];
        // Frame header descriptor: Single_Segment set, so there is no window
        // descriptor and the content size that follows is one byte.
        out.push(0x20);
        out.push(content.len() as u8);
        // Block header, 3 bytes little-endian: last-block bit, then a 2-bit type
        // (0 = Raw), then the size.
        let header = 1u32 | ((content.len() as u32) << 3);
        out.extend_from_slice(&header.to_le_bytes()[..3]);
        out.extend_from_slice(content);
        out
    }

    fn skippable_frame(content: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&SKIPPABLE.start().to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        out.extend_from_slice(content);
        out
    }

    fn read_archive(bytes: &[u8]) -> String {
        let dir = std::env::temp_dir().join("blindfold-curate-dump-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        // Named for the content so concurrent tests cannot collide on one file.
        let path = dir.join(format!(
            "{:x}.zst",
            bytes.len() as u64 * 31 + bytes[0] as u64
        ));
        std::fs::write(&path, bytes).expect("write fixture");
        let mut archive = Archive::open(path.to_str().expect("utf-8 path")).expect("open");
        let mut out = String::new();
        archive.read_to_string(&mut out).expect("read");
        out
    }

    #[test]
    fn reads_a_plain_single_frame_archive() {
        assert_eq!(read_archive(&data_frame(b"a,b,c\n")), "a,b,c\n");
    }

    /// The bug that blocked curation: the Lichess dump opens with a 12-byte
    /// skippable frame, and handing it straight to `StreamingDecoder` fails with
    /// `SkipFrame { magic_number: 0x184D2A50 }`.
    #[test]
    fn a_leading_skippable_frame_is_skipped() {
        let mut bytes = skippable_frame(b"metadata");
        bytes.extend(data_frame(b"a,b,c\n"));
        assert_eq!(read_archive(&bytes), "a,b,c\n");
    }

    /// The reason this module is not `seek(12)`. A single-frame decoder reports EOF
    /// at the end of frame one, so the second frame's rows would vanish with no
    /// error anywhere — a silently truncated database that looks merely small.
    #[test]
    fn every_data_frame_is_read_not_just_the_first() {
        let mut bytes = data_frame(b"first\n");
        bytes.extend(data_frame(b"second\n"));
        assert_eq!(read_archive(&bytes), "first\nsecond\n");
    }

    #[test]
    fn skippable_frames_between_and_after_data_frames_are_skipped() {
        let mut bytes = skippable_frame(b"lead");
        bytes.extend(data_frame(b"first\n"));
        bytes.extend(skippable_frame(b"middle"));
        bytes.extend(data_frame(b"second\n"));
        bytes.extend(skippable_frame(b"trail"));
        assert_eq!(read_archive(&bytes), "first\nsecond\n");
    }

    /// Every magic in the range is skippable — the low nibble is the writer's to
    /// use, so matching only `0x184D2A50` would reject a legal archive.
    #[test]
    fn the_whole_skippable_magic_range_is_recognised() {
        for magic in SKIPPABLE {
            let mut bytes = magic.to_le_bytes().to_vec();
            bytes.extend_from_slice(&4u32.to_le_bytes());
            bytes.extend_from_slice(b"meta");
            bytes.extend(data_frame(b"x\n"));
            let dir = std::env::temp_dir().join("blindfold-curate-dump-tests");
            std::fs::create_dir_all(&dir).expect("temp dir");
            let path = dir.join(format!("magic-{magic:x}.zst"));
            std::fs::write(&path, &bytes).expect("write fixture");
            let mut archive = Archive::open(path.to_str().expect("utf-8 path")).expect("open");
            let mut out = String::new();
            archive.read_to_string(&mut out).expect("read");
            assert_eq!(out, "x\n", "magic {magic:#x} should be skippable");
        }
    }

    /// The hand-built frames above prove the layout logic; only the real dump proves
    /// it against a 302 MB file zstd actually produced. Ignored by default because
    /// the dump is 300 MB and deliberately not committed.
    ///
    /// ```text
    /// BLINDFOLD_DUMP=/path/to/lichess_db_puzzle.csv.zst \
    ///   cargo test -p blindfold-curate -- --ignored --nocapture
    /// ```
    ///
    /// Asserts the *line count*, not merely that reading returns `Ok`. Silent
    /// truncation is by definition an `Ok` that stops early, so a smoke test that
    /// only checks for an error is exactly the test that cannot catch it.
    #[test]
    #[ignore = "needs the 300 MB dump; set BLINDFOLD_DUMP"]
    fn the_real_dump_reads_every_line() {
        let Ok(path) = std::env::var("BLINDFOLD_DUMP") else {
            panic!("set BLINDFOLD_DUMP to the lichess_db_puzzle.csv.zst path");
        };
        let archive = Archive::open(&path).expect("open dump");
        let lines = std::io::BufRead::lines(std::io::BufReader::new(archive)).count();
        println!("{lines} lines");
        assert_eq!(lines, EXPECTED_DUMP_LINES);
    }

    /// Line count of the 2026-07-05 dump, cross-checked against python-zstandard —
    /// an independent decoder, so this pins ruzstd's output rather than restating it.
    /// A newer dump will fail this; update it and say which dump, don't delete it.
    const EXPECTED_DUMP_LINES: usize = 6_057_357;

    /// Truncation must be an error, never a short read. This is the failure the
    /// whole module exists to make impossible.
    #[test]
    fn a_truncated_frame_is_an_error() {
        let full = data_frame(b"a,b,c\n");
        let cut = &full[..full.len() - 2];
        let dir = std::env::temp_dir().join("blindfold-curate-dump-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("truncated.zst");
        std::fs::write(&path, cut).expect("write fixture");
        let mut archive = Archive::open(path.to_str().expect("utf-8 path")).expect("open");
        let mut out = String::new();
        assert!(archive.read_to_string(&mut out).is_err());
    }
}
