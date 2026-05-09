//! Sfnt2Woff: convert TTF/OTF (sfnt) fonts to WOFF format.
//!
//! Native Rust port of the Go implementation (which itself was ported from
//! <https://github.com/fontello/ttf2woff>).
//!
//! Compression compatibility note:
//! The Go implementation writes each table by calling
//! `zlib.Writer.{Write, Flush, Close}` on a level-6 zlib encoder. We use
//! the d2-flate crate (a pure-Rust port of Go's compress/flate) to produce
//! byte-identical zlib output.

// SFNT table-directory field offsets (within each 16-byte entry)
const SFNT_OFFSET_TAG: usize = 0;
const SFNT_OFFSET_CHECKSUM: usize = 4;
const SFNT_OFFSET_OFFSET: usize = 8;
const SFNT_OFFSET_LENGTH: usize = 12;

// SFNT "head" table internal offsets
const SFNT_ENTRY_OFFSET_FLAVOR: usize = 0;
const SFNT_ENTRY_OFFSET_VERSION_MAJ: usize = 4;
const SFNT_ENTRY_OFFSET_VERSION_MIN: usize = 6;
const SFNT_ENTRY_OFFSET_CHECKSUM_ADJUSTMENT: usize = 8;

// WOFF header field offsets
const WOFF_OFFSET_MAGIC: usize = 0;
const WOFF_OFFSET_FLAVOR: usize = 4;
const WOFF_OFFSET_SIZE: usize = 8;
const WOFF_OFFSET_NUM_TABLES: usize = 12;
// const WOFF_OFFSET_RESERVED: usize = 14;  // always 0
const WOFF_OFFSET_SFNT_SIZE: usize = 16;
const WOFF_OFFSET_VERSION_MAJ: usize = 20;
const WOFF_OFFSET_VERSION_MIN: usize = 22;
const WOFF_OFFSET_META_OFFSET: usize = 24;
const WOFF_OFFSET_META_LENGTH: usize = 28;
const WOFF_OFFSET_META_ORIG_LENGTH: usize = 32;
const WOFF_OFFSET_PRIV_OFFSET: usize = 36;
const WOFF_OFFSET_PRIV_LENGTH: usize = 40;

// WOFF table-directory entry field offsets
const WOFF_ENTRY_OFFSET_TAG: usize = 0;
const WOFF_ENTRY_OFFSET_OFFSET: usize = 4;
const WOFF_ENTRY_OFFSET_COMPR_LENGTH: usize = 8;
const WOFF_ENTRY_OFFSET_LENGTH: usize = 12;
const WOFF_ENTRY_OFFSET_CHECKSUM: usize = 16;

// Magic numbers
const MAGIC_WOFF: u32 = 0x774F4646;
const MAGIC_CHECKSUM_ADJUSTMENT: u32 = 0xB1B0AFBA;

// Sizes
const SIZE_OF_WOFF_HEADER: usize = 44;
const SIZE_OF_WOFF_ENTRY: usize = 20;
const SIZE_OF_SFNT_HEADER: usize = 12;
const SIZE_OF_SFNT_TABLE_ENTRY: usize = 16;

struct TableEntry {
    tag: [u8; 4],
    checksum: u32,
    offset: u32,
    length: u32,
}

/// Round up to next 4-byte boundary.
fn long_align(n: u32) -> u32 {
    (n + 3) & !3
}

/// Calculate a 32-bit checksum over `buf` (treating it as big-endian u32 words).
fn calc_checksum(buf: &[u8]) -> u32 {
    let nlongs = buf.len() / 4;
    let mut sum: u32 = 0;
    for i in 0..nlongs {
        let t = u32::from_be_bytes([buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]]);
        sum = sum.wrapping_add(t);
    }
    sum
}

fn read_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

fn write_u16(buf: &mut [u8], offset: usize, val: u16) {
    let bytes = val.to_be_bytes();
    buf[offset] = bytes[0];
    buf[offset + 1] = bytes[1];
}

fn write_u32(buf: &mut [u8], offset: usize, val: u32) {
    let bytes = val.to_be_bytes();
    buf[offset] = bytes[0];
    buf[offset + 1] = bytes[1];
    buf[offset + 2] = bytes[2];
    buf[offset + 3] = bytes[3];
}

/// Compress `input` with zlib level 6 using the same output framing Go's
/// `compress/zlib.Writer` produces when the caller invokes
/// `Write` + `Flush` + `Close`. Implemented in d2-flate (a pure-Rust port
/// of Go's compress/flate) so the deflate symbol stream is byte-identical
/// to Go's, not just spec-compliant.
fn zlib_compress_go_compat(input: &[u8]) -> Result<Vec<u8>, String> {
    Ok(crate::flate::zlib_compress_level6_go_compat(input))
}

/// Convert an SFNT font buffer (TTF or OTF) to WOFF format.
pub fn sfnt2woff(font_buf: &[u8]) -> Result<Vec<u8>, String> {
    let num_tables = read_u16(font_buf, 4) as usize;

    // -- build WOFF header (44 bytes) --
    let mut woff_header = vec![0u8; SIZE_OF_WOFF_HEADER];
    write_u32(&mut woff_header, WOFF_OFFSET_MAGIC, MAGIC_WOFF);
    write_u16(&mut woff_header, WOFF_OFFSET_NUM_TABLES, num_tables as u16);
    // reserved, meta, priv fields all stay 0
    write_u16(&mut woff_header, WOFF_OFFSET_SFNT_SIZE, 0);
    write_u32(&mut woff_header, WOFF_OFFSET_META_OFFSET, 0);
    write_u32(&mut woff_header, WOFF_OFFSET_META_LENGTH, 0);
    write_u32(&mut woff_header, WOFF_OFFSET_META_ORIG_LENGTH, 0);
    write_u32(&mut woff_header, WOFF_OFFSET_PRIV_OFFSET, 0);
    write_u32(&mut woff_header, WOFF_OFFSET_PRIV_LENGTH, 0);

    // -- read SFNT table directory entries --
    let mut entries: Vec<TableEntry> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let base = SIZE_OF_SFNT_HEADER + i * SIZE_OF_SFNT_TABLE_ENTRY;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&font_buf[base + SFNT_OFFSET_TAG..base + SFNT_OFFSET_TAG + 4]);
        entries.push(TableEntry {
            tag,
            checksum: read_u32(font_buf, base + SFNT_OFFSET_CHECKSUM),
            offset: read_u32(font_buf, base + SFNT_OFFSET_OFFSET),
            length: read_u32(font_buf, base + SFNT_OFFSET_LENGTH),
        });
    }

    // Sort entries by tag (alphabetical)
    entries.sort_by_key(|entry| entry.tag);

    // -- verify checksums & populate WOFF table info --
    let mut sfnt_size = (SIZE_OF_SFNT_HEADER + num_tables * SIZE_OF_SFNT_TABLE_ENTRY) as u32;
    let mut table_info = vec![0u8; num_tables * SIZE_OF_WOFF_ENTRY];

    for (i, entry) in entries.iter().enumerate() {
        if &entry.tag != b"head" {
            let end = entry.offset + long_align(entry.length);
            let align_table = &font_buf[entry.offset as usize..end as usize];
            if calc_checksum(align_table) != entry.checksum {
                return Err(format!(
                    "checksum error in table: {}",
                    String::from_utf8_lossy(&entry.tag)
                ));
            }
        }

        let base = i * SIZE_OF_WOFF_ENTRY;
        write_u32(
            &mut table_info,
            base + WOFF_ENTRY_OFFSET_TAG,
            u32::from_be_bytes(entry.tag),
        );
        write_u32(
            &mut table_info,
            base + WOFF_ENTRY_OFFSET_LENGTH,
            entry.length,
        );
        write_u32(
            &mut table_info,
            base + WOFF_ENTRY_OFFSET_CHECKSUM,
            entry.checksum,
        );

        sfnt_size += long_align(entry.length);
    }

    // -- compute SFNT checksum adjustment --
    let mut sfnt_offset = (SIZE_OF_SFNT_HEADER + entries.len() * SIZE_OF_SFNT_TABLE_ENTRY) as u32;
    let mut csum = calc_checksum(&font_buf[..SIZE_OF_SFNT_HEADER]);
    for entry in &entries {
        let mut b = [0u8; SIZE_OF_SFNT_TABLE_ENTRY];
        write_u32(&mut b, SFNT_OFFSET_TAG, u32::from_be_bytes(entry.tag));
        write_u32(&mut b, SFNT_OFFSET_CHECKSUM, entry.checksum);
        write_u32(&mut b, SFNT_OFFSET_OFFSET, sfnt_offset);
        write_u32(&mut b, SFNT_OFFSET_LENGTH, entry.length);

        sfnt_offset += long_align(entry.length);
        csum = csum.wrapping_add(calc_checksum(&b));
        csum = csum.wrapping_add(entry.checksum);
    }
    let checksum_adjustment = MAGIC_CHECKSUM_ADJUSTMENT.wrapping_sub(csum);

    // -- compress tables & build output --
    let mut major_version: u16 = 0;
    let mut min_version: u16 = 1;
    let mut flavor: u32 = 0;
    let mut offset = SIZE_OF_WOFF_HEADER + num_tables * SIZE_OF_WOFF_ENTRY;
    let mut table_bytes: Vec<u8> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let sfnt_data_start = entry.offset as usize;
        let sfnt_data_end = sfnt_data_start + entry.length as usize;
        // We need a mutable copy for the "head" table
        let mut sfnt_data = font_buf[sfnt_data_start..sfnt_data_end].to_vec();

        if &entry.tag == b"head" {
            major_version = read_u16(&sfnt_data, SFNT_ENTRY_OFFSET_VERSION_MAJ);
            min_version = read_u16(&sfnt_data, SFNT_ENTRY_OFFSET_VERSION_MIN);
            flavor = read_u32(&sfnt_data, SFNT_ENTRY_OFFSET_FLAVOR);
            write_u32(
                &mut sfnt_data,
                SFNT_ENTRY_OFFSET_CHECKSUM_ADJUSTMENT,
                checksum_adjustment,
            );
        }

        // zlib compress, mirroring Go's `w.Write ; w.Flush ; w.Close` pattern
        // (level 6 + Z_SYNC_FLUSH + Z_FINISH). Including the sync-flush
        // framing bloats small-table output enough that both sides pick the
        // "store uncompressed" branch, which is exactly what Go does.
        let compressed = zlib_compress_go_compat(&sfnt_data)?;

        // Only use compression if it actually saves space
        let comp_length = compressed.len().min(sfnt_data.len());
        let aligned_length = long_align(comp_length as u32) as usize;

        let mut table = vec![0u8; aligned_length];
        if compressed.len() >= sfnt_data.len() {
            table[..sfnt_data.len()].copy_from_slice(&sfnt_data);
        } else {
            table[..compressed.len()].copy_from_slice(&compressed);
        }

        let base = i * SIZE_OF_WOFF_ENTRY;
        write_u32(
            &mut table_info,
            base + WOFF_ENTRY_OFFSET_OFFSET,
            offset as u32,
        );
        offset += table.len();
        write_u32(
            &mut table_info,
            base + WOFF_ENTRY_OFFSET_COMPR_LENGTH,
            comp_length as u32,
        );

        table_bytes.extend_from_slice(&table);
    }

    // -- finalize WOFF header --
    let woff_size = (woff_header.len() + table_info.len() + table_bytes.len()) as u32;
    write_u32(&mut woff_header, WOFF_OFFSET_SIZE, woff_size);
    write_u32(&mut woff_header, WOFF_OFFSET_SFNT_SIZE, sfnt_size);
    write_u16(&mut woff_header, WOFF_OFFSET_VERSION_MAJ, major_version);
    write_u16(&mut woff_header, WOFF_OFFSET_VERSION_MIN, min_version);
    write_u32(&mut woff_header, WOFF_OFFSET_FLAVOR, flavor);

    // -- assemble output --
    let mut out = Vec::with_capacity(woff_size as usize);
    out.extend_from_slice(&woff_header);
    out.extend_from_slice(&table_info);
    out.extend_from_slice(&table_bytes);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_align() {
        assert_eq!(long_align(0), 0);
        assert_eq!(long_align(1), 4);
        assert_eq!(long_align(2), 4);
        assert_eq!(long_align(3), 4);
        assert_eq!(long_align(4), 4);
        assert_eq!(long_align(5), 8);
    }

    #[test]
    fn test_calc_checksum() {
        let buf = [0u8; 8];
        assert_eq!(calc_checksum(&buf), 0);

        let mut buf = [0u8; 4];
        buf[0] = 0x00;
        buf[1] = 0x00;
        buf[2] = 0x00;
        buf[3] = 0x01;
        assert_eq!(calc_checksum(&buf), 1);
    }
}
