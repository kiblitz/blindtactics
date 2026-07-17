//! Reading the `.csv.zst` dump.
//!
//! # Why this is not one line of ruzstd
//!
//! `ruzstd::StreamingDecoder` decodes exactly **one** zstd frame and, by its own
//! documentation, expects the stream to contain only that. Handed the dump directly
//! it fails immediately with `SkipFrame { magic_number: 0x184D2A50 }`.
//!
//! The tempting fix is to seek past that first frame — the real data does start at
//! byte 12 — and carry on. **That silently discards 97% of the dump.**
//!
//! The file is `pzstd` output: **34 data frames, each preceded by its own 12-byte
//! skippable frame** (4 magic, 4 length, a 4-byte payload) whose payload is the
//! compressed size of the frame that follows. Frame 1 holds 32 MiB — 182,109 rows of
//! 6,057,357. So a seek-and-decode-one-frame reader reports a clean EOF after 3.01%
//! of the file, with no error anywhere.
//!
//! So this walks frames properly: skip the skippable ones, decode each data frame in
//! turn, and stop only at real end-of-file. All of the above is measured, not
//! inferred — walking the frame table lands exactly on the 302,111,223-byte file
//! size, and `the_real_dump_reads_every_line` reads all 6,057,357 rows.
//!
//! Note it is *`pzstd`* that produces this layout, not `zstd -T0` or `--adapt`:
//! libzstd's multithreading splits input into jobs but concatenates them as blocks
//! **inside a single frame**, so those flags are a red herring. `cat a.zst b.zst`
//! and the seekable format are the other multi-frame producers.

use std::io::Read as _;
use std::io::Seek as _;

/// Zstd's skippable-frame magic numbers. The low nibble is free for the writer to
/// use, so the whole range means "skippable" (RFC 8878: "all 16 values are valid").
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
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
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
            // A frame boundary that is also EOF — the one clean place to stop.
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
        let length = u32::from_le_bytes(length);

        // Bounds-check the skip rather than seeking blind. Seeking past EOF succeeds
        // on every platform, so a corrupt length would land the source past the end,
        // the next `read_exact` would return `UnexpectedEof`, and the arm above would
        // read that as a clean end of file — `Ok("")`, no error, every remaining frame
        // silently gone. That is exactly the failure this module exists to prevent.
        let end = source.get_ref().metadata()?.len();
        let landed = source.seek(std::io::SeekFrom::Current(i64::from(length)))?;
        if landed > end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "skippable frame declares {length} bytes, which runs {} past the \
                     end of the file",
                    landed - end
                ),
            ));
        }
    }
}

impl std::io::Read for Archive {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // `Read`'s contract makes an empty buffer a no-op, and honouring it here is
        // load-bearing rather than pedantic: `StreamingDecoder::read` also returns
        // `Ok(0)` for an empty buffer, and the loop below reads `Ok(0)` as "this frame
        // is spent". Without this, a zero-length read tears the decoder off a
        // half-read frame and hands its content bytes to `next_frame` as a magic
        // number.
        if buf.is_empty() {
            return Ok(0);
        }
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

/// These live here rather than in `tests/` because they reach `MAGIC_BYTES`, which is
/// private to this module. The lib target does make `dump` importable from `tests/` —
/// `select` is tested that way — so the reason is the private access, nothing about
/// the crate's shape.
#[cfg(test)]
mod tests {
    use crate::dump;
    use std::io::Read as _;

    /// Where fixture archives are written. Each test names its own file: sharing one
    /// path would let concurrently-running tests clobber each other's bytes.
    const FIXTURE_DIR: &str = "blindfold-curate-dump-tests";

    /// Zstd's data-frame magic, little-endian.
    const DATA_FRAME_MAGIC: u32 = 0xFD2F_B528;

    /// The skippable range per RFC 8878, as an independent second copy of
    /// `dump::SKIPPABLE`.
    ///
    /// Deliberately not read from `dump`. Deriving the fixtures from the constant
    /// under test is what made the first version of
    /// `the_whole_skippable_magic_range_is_recognised` vacuous: it iterated
    /// `SKIPPABLE` *and* built its bytes from `SKIPPABLE.start()`, so both moved
    /// together and the whole suite stayed green with the range set to values that
    /// fail on the real dump. A fixture must never move with what it checks.
    const SPEC_SKIPPABLE_FIRST: u32 = 0x184D_2A50;
    const SPEC_SKIPPABLE_LAST: u32 = 0x184D_2A5F;

    /// A zstd data frame holding `content` as a single uncompressed block.
    ///
    /// Hand-built rather than compressed with a library, because the point is to
    /// control the *frame layout* — how many frames and in what order — which a
    /// compressor picks for you. Layout per RFC 8878.
    fn data_frame(content: &[u8]) -> Vec<u8> {
        assert!(
            content.len() <= u8::MAX as usize,
            "1-byte content size field"
        );
        let mut out = DATA_FRAME_MAGIC.to_le_bytes().to_vec();
        // Frame header descriptor: Single_Segment (bit 5) set, so there is no window
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

    fn skippable_frame_with_magic(magic: u32, content: &[u8]) -> Vec<u8> {
        let mut out = magic.to_le_bytes().to_vec();
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        out.extend_from_slice(content);
        out
    }

    fn skippable_frame(content: &[u8]) -> Vec<u8> {
        skippable_frame_with_magic(SPEC_SKIPPABLE_FIRST, content)
    }

    /// Write `bytes` to a file named for the calling test, and open it.
    fn archive_of(name: &str, bytes: &[u8]) -> std::io::Result<dump::Archive> {
        let dir = std::env::temp_dir().join(FIXTURE_DIR);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(format!("{name}.zst"));
        std::fs::write(&path, bytes).expect("write fixture");
        dump::Archive::open(&path)
    }

    /// Read a whole archive, requiring success.
    fn read_archive(name: &str, bytes: &[u8]) -> String {
        let mut out = String::new();
        archive_of(name, bytes)
            .expect("open")
            .read_to_string(&mut out)
            .expect("read");
        out
    }

    /// Read a whole archive, surfacing an error from *either* stage.
    ///
    /// The frame walk begins in `Archive::open` — it must, since opening means
    /// finding the first data frame — so a corrupt archive can be rejected before any
    /// `read` call. A helper that unwrapped `open` would turn the very failures the
    /// error tests are asserting into panics.
    fn try_read_archive(name: &str, bytes: &[u8]) -> std::io::Result<String> {
        let mut out = String::new();
        archive_of(name, bytes)?.read_to_string(&mut out)?;
        Ok(out)
    }

    #[test]
    fn reads_a_plain_single_frame_archive() {
        let bytes = data_frame(b"a,b,c\n");
        assert_eq!(read_archive("plain", &bytes), "a,b,c\n");
    }

    /// The bug that blocked curation: the dump opens with a skippable frame, and
    /// handing it straight to `StreamingDecoder` fails with
    /// `SkipFrame { magic_number: 0x184D2A50 }`.
    #[test]
    fn a_leading_skippable_frame_is_skipped() {
        let mut bytes = skippable_frame(b"metadata");
        bytes.extend(data_frame(b"a,b,c\n"));
        assert_eq!(read_archive("leading-skippable", &bytes), "a,b,c\n");
    }

    /// The reason this module is not `seek(12)`. The real dump is 34 data frames, so
    /// a single-frame decoder reports a clean EOF after 3% of the rows, having
    /// silently dropped the other 97%.
    #[test]
    fn every_data_frame_is_read_not_just_the_first() {
        let mut bytes = data_frame(b"first\n");
        bytes.extend(data_frame(b"second\n"));
        assert_eq!(read_archive("two-data-frames", &bytes), "first\nsecond\n");
    }

    #[test]
    fn skippable_frames_between_and_after_data_frames_are_skipped() {
        let mut bytes = skippable_frame(b"lead");
        bytes.extend(data_frame(b"first\n"));
        bytes.extend(skippable_frame(b"middle"));
        bytes.extend(data_frame(b"second\n"));
        bytes.extend(skippable_frame(b"trail"));
        assert_eq!(read_archive("interleaved", &bytes), "first\nsecond\n");
    }

    /// Every magic in the range is skippable — the low nibble is the writer's to use,
    /// so matching only `0x184D2A50` would reject a legal archive.
    ///
    /// Iterates the *spec's* range, not `dump::SKIPPABLE`; see `SPEC_SKIPPABLE_FIRST`.
    #[test]
    fn the_whole_skippable_magic_range_is_recognised() {
        for magic in SPEC_SKIPPABLE_FIRST..=SPEC_SKIPPABLE_LAST {
            let mut bytes = skippable_frame_with_magic(magic, b"meta");
            bytes.extend(data_frame(b"x\n"));
            let out = read_archive(&format!("magic-{magic:x}"), &bytes);
            assert_eq!(out, "x\n", "magic {magic:#x} should be skippable");
        }
    }

    /// The other half, and the half that catches an over-wide range: a magic just
    /// outside the range is not skippable and is not a valid data frame, so it must
    /// be an error rather than something we quietly step over.
    #[test]
    fn magics_outside_the_skippable_range_are_not_skipped() {
        for magic in [SPEC_SKIPPABLE_FIRST - 1, SPEC_SKIPPABLE_LAST + 1] {
            let bytes = skippable_frame_with_magic(magic, b"meta");
            let result = try_read_archive(&format!("outside-{magic:x}"), &bytes);
            assert!(result.is_err(), "magic {magic:#x} must not be skipped");
        }
    }

    /// Truncation must be an error, never a short read. This covers the *data frame*
    /// path, where ruzstd does the detecting.
    #[test]
    fn a_truncated_data_frame_is_an_error() {
        let full = data_frame(b"a,b,c\n");
        let result = try_read_archive("truncated-data", &full[..full.len() - 2]);
        assert!(result.is_err());
    }

    /// The same guarantee on the *skippable* path, which is hand-written here rather
    /// than delegated to ruzstd — and which had exactly the bug this module exists to
    /// prevent. Seeking past EOF succeeds on every platform, so the following
    /// `read_exact` returned `UnexpectedEof`, which `next_frame` read as a clean end
    /// of file: `Ok("")`, no error, every remaining frame gone.
    #[test]
    fn a_skippable_frame_whose_length_overruns_eof_is_an_error() {
        let mut bytes = skippable_frame_with_magic(SPEC_SKIPPABLE_FIRST, b"meta");
        // Claim a payload far larger than what actually follows.
        bytes[dump::MAGIC_BYTES..2 * dump::MAGIC_BYTES].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend(data_frame(b"payload\n"));

        let result = try_read_archive("overlong-skippable", &bytes);
        assert!(
            result.is_err(),
            "a skippable length running past EOF is corruption, not a frame boundary; \
             got {result:?}"
        );
    }

    /// `Read`'s contract: a zero-length buffer is a no-op returning `Ok(0)`. It must
    /// not be mistaken for "this frame is spent" — `StreamingDecoder::read` also
    /// returns `Ok(0)` for an empty buffer, so treating that as EOF tore the source
    /// apart mid-frame and handed content bytes to `next_frame` as a magic number.
    #[test]
    fn an_empty_buffer_read_does_not_consume_a_frame() {
        let mut bytes = data_frame(b"first\n");
        bytes.extend(data_frame(b"second\n"));
        let mut archive = archive_of("empty-buf", &bytes).expect("open");

        assert_eq!(archive.read(&mut []).expect("empty read is a no-op"), 0);

        let mut out = String::new();
        archive.read_to_string(&mut out).expect("read after empty");
        assert_eq!(
            out, "first\nsecond\n",
            "the empty read must not have consumed"
        );
    }

    /// The hand-built frames above prove the layout logic; only the real dump proves
    /// it against the 302 MB file `pzstd` actually produced. Ignored by default
    /// because the dump is 300 MB and deliberately not committed.
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
        let archive = dump::Archive::open(&path).expect("open dump");
        let lines = std::io::BufRead::lines(std::io::BufReader::new(archive)).count();
        println!("{lines} lines");
        assert_eq!(lines, EXPECTED_DUMP_LINES);
    }

    /// Line count of the 2026-07-05 dump, cross-checked against python-zstandard —
    /// an independent decoder, so this pins ruzstd's output rather than restating it.
    /// A newer dump will fail this; update it and say which dump, don't delete it.
    const EXPECTED_DUMP_LINES: usize = 6_057_357;
}
