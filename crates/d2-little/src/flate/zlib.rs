//! zlib (RFC 1950) framing around the deflate stream.
//!
//! Faithful port of Go's `compress/zlib/writer.go` for the
//! `Write` + `Flush` + `Close` sequence at level 6 with no dictionary —
//! exactly the pattern d2-font's `sfnt2woff` uses for each font table.

use crate::flate::adler32::Adler32;
use crate::flate::deflate::Compressor;

/// Compress `input` to a zlib stream using level 6 with the same flush
/// pattern Go's `compress/zlib.Writer` produces when callers invoke
/// `Write` + `Flush` + `Close`:
///
/// 1. zlib 2-byte header (0x78 0x9c — CMF + FLG with FLEVEL=2 for level 6).
/// 2. Deflate stream over `input`, capped with a `Z_SYNC_FLUSH` marker
///    (empty stored non-final block) followed by a `Z_FINISH` marker
///    (empty stored final block).
/// 3. Adler-32 trailer over the original input, big-endian.
///
/// This matches Go's compress/flate output byte-for-byte for our font
/// pipeline (provided the deflate engine itself is also byte-identical,
/// which is what this whole crate exists to guarantee).
pub fn zlib_compress_level6_go_compat(input: &[u8]) -> Vec<u8> {
    let mut comp = Compressor::new_level6();

    // Write the zlib header. Mirrors writer.go writeHeader for level 6:
    // CMF = 0x78 (CINFO=7 << 4 | CM=8 = 0x78); FLG = 2<<6 then add (31 - mod31)
    // checksum. For 0x78 0x?? without a dictionary the standard FLG is 0x9C
    // (level 6).
    let cmf: u8 = 0x78;
    let flg_base: u8 = 2 << 6; // FLEVEL=2, FDICT=0
    let cmf_flg = ((cmf as u16) << 8) | (flg_base as u16);
    let flg = flg_base + (31 - (cmf_flg % 31)) as u8;
    comp.w.out.push(cmf);
    comp.w.out.push(flg);

    // Phase 1: feed input. Go's zlib.Writer.Write -> compressor.write does
    // not flush, so we just call write here.
    comp.write(input);

    // Phase 2: Z_SYNC_FLUSH (Go's zlib.Writer.Flush -> compressor.syncFlush).
    comp.sync_flush();

    // Phase 3: Z_FINISH (Go's zlib.Writer.Close -> compressor.close).
    comp.close();

    // Adler-32 trailer (big-endian) over the original input.
    let checksum = Adler32::new().update(input).finalize();
    comp.w.out.push((checksum >> 24) as u8);
    comp.w.out.push((checksum >> 16) as u8);
    comp.w.out.push((checksum >> 8) as u8);
    comp.w.out.push(checksum as u8);

    std::mem::take(&mut comp.w.out)
}
