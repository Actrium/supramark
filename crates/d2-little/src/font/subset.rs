//! Font subsetting: extract only the glyphs needed for a given text corpus.
//!
//! Ported from Go `lib/font/subsetFont.go` (originally from gofpdf).

use std::collections::BTreeMap;

type ParsedSymbols = (
    BTreeMap<usize, usize>, // rune_symbol_pair
    BTreeMap<usize, usize>, // symbol_array
    BTreeMap<usize, usize>, // symbol_collection
    Vec<usize>,             // symbol_collection_keys
);

// Composite-glyph flags
const SYMBOL_WORDS: u16 = 1 << 0;
const SYMBOL_SCALE: u16 = 1 << 3;
const SYMBOL_CONTINUE: u16 = 1 << 5;
const SYMBOL_ALL_SCALE: u16 = 1 << 6;
const SYMBOL_2X2: u16 = 1 << 7;

/// A simple cursor reader over a byte buffer.
struct FontReader {
    data: Vec<u8>,
    pos: usize,
}

impl FontReader {
    fn new(data: Vec<u8>) -> Self {
        Self { data, pos: 0 }
    }

    fn read(&mut self, n: usize) -> &[u8] {
        let start = self.pos;
        self.pos += n;
        &self.data[start..self.pos]
    }

    fn seek_abs(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    fn read_u16(&mut self) -> u16 {
        let b = self.read(2);
        ((b[0] as u16) << 8) | (b[1] as u16)
    }

    fn read_u32(&mut self) -> u32 {
        let b = self.read(4);
        ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
    }

    fn read_i16(&mut self) -> i16 {
        let val = self.read_u16();
        val as i16
    }

    fn get_u16_at(&mut self, pos: usize) -> u16 {
        self.seek_abs(pos);
        self.read_u16()
    }

    fn get_range(&mut self, pos: usize, length: usize) -> Vec<u8> {
        self.seek_abs(pos);
        if length < 1 {
            return Vec::new();
        }
        self.read(length).to_vec()
    }

    fn read_table_name(&mut self) -> String {
        let b = self.read(4);
        String::from_utf8_lossy(b).to_string()
    }
}

struct TableDesc {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    checksum: [u16; 2],
    position: usize,
    size: usize,
}

struct Utf8FontFile {
    reader: FontReader,
    last_rune: usize,
    table_descs: BTreeMap<String, TableDesc>,
    out_tables: BTreeMap<String, Vec<u8>>,
    symbol_position: Vec<usize>,
    char_symbol_dict: BTreeMap<usize, usize>,
    #[allow(dead_code)]
    symbol_data: BTreeMap<usize, BTreeMap<String, Vec<usize>>>,
    code_symbol_dict: BTreeMap<usize, usize>,
}

impl Utf8FontFile {
    fn new(data: Vec<u8>) -> Self {
        Self {
            reader: FontReader::new(data),
            last_rune: 0,
            table_descs: BTreeMap::new(),
            out_tables: BTreeMap::new(),
            symbol_position: Vec::new(),
            char_symbol_dict: BTreeMap::new(),
            symbol_data: BTreeMap::new(),
            code_symbol_dict: BTreeMap::new(),
        }
    }

    fn generate_table_descriptions(&mut self) {
        let tables_count = self.reader.read_u16() as usize;
        let _ = self.reader.read_u16(); // searchRange
        let _ = self.reader.read_u16(); // entrySelector
        let _ = self.reader.read_u16(); // rangeShift
        self.table_descs = BTreeMap::new();
        for _ in 0..tables_count {
            let name = self.reader.read_table_name();
            let c0 = self.reader.read_u16();
            let c1 = self.reader.read_u16();
            let position = self.reader.read_u32() as usize;
            let size = self.reader.read_u32() as usize;
            self.table_descs.insert(
                name.clone(),
                TableDesc {
                    name,
                    checksum: [c0, c1],
                    position,
                    size,
                },
            );
        }
    }

    fn seek_table(&mut self, name: &str, offset: usize) -> usize {
        let pos = self.table_descs[name].position + offset;
        self.reader.seek_abs(pos);
        pos
    }

    fn get_table_data(&mut self, name: &str) -> Option<Vec<u8>> {
        let desc = self.table_descs.get(name)?;
        if desc.size == 0 {
            return None;
        }
        let pos = desc.position;
        let size = desc.size;
        Some(self.reader.get_range(pos, size))
    }

    fn set_out_table(&mut self, name: &str, data: Option<Vec<u8>>) {
        let Some(mut data) = data else { return };
        if name == "head" {
            // Zero out checksumAdjustment in head table
            if data.len() > 11 {
                data[8] = 0;
                data[9] = 0;
                data[10] = 0;
                data[11] = 0;
            }
        }
        self.out_tables.insert(name.to_string(), data);
    }

    fn generate_cmap(&mut self) -> Option<BTreeMap<usize, Vec<usize>>> {
        let cmap_pos = self.seek_table("cmap", 0);
        self.reader.skip(2);
        let cmap_table_count = self.reader.read_u16() as usize;
        let mut rune_cmap_pos = 0usize;

        for _ in 0..cmap_table_count {
            let system = self.reader.read_u16();
            let coder = self.reader.read_u16();
            let position = self.reader.read_u32() as usize;
            let old_pos = self.reader.pos;
            if (system == 3 && coder == 1) || system == 0 {
                let format = self.reader.get_u16_at(cmap_pos + position);
                if format == 4 {
                    rune_cmap_pos = cmap_pos + position;
                    break;
                }
            }
            self.reader.seek_abs(old_pos);
        }

        if rune_cmap_pos == 0 {
            return None;
        }

        let mut symbol_char_dict: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut char_symbol_dict: BTreeMap<usize, usize> = BTreeMap::new();
        self.generate_sccs_dictionaries(
            rune_cmap_pos,
            &mut symbol_char_dict,
            &mut char_symbol_dict,
        );
        self.char_symbol_dict = char_symbol_dict;
        Some(symbol_char_dict)
    }

    fn generate_sccs_dictionaries(
        &mut self,
        rune_cmap_pos: usize,
        symbol_char_dict: &mut BTreeMap<usize, Vec<usize>>,
        char_symbol_dict: &mut BTreeMap<usize, usize>,
    ) {
        self.reader.seek_abs(rune_cmap_pos + 2);
        let size = self.reader.read_u16() as usize;
        let rim = rune_cmap_pos + size;
        self.reader.skip(2);

        let segment_size = self.reader.read_u16() as usize / 2;
        self.reader.skip(6);

        let mut completers = Vec::with_capacity(segment_size);
        for _ in 0..segment_size {
            completers.push(self.reader.read_u16() as usize);
        }
        self.reader.skip(2);

        let mut beginners = Vec::with_capacity(segment_size);
        for _ in 0..segment_size {
            beginners.push(self.reader.read_u16() as usize);
        }

        let mut sizes = Vec::with_capacity(segment_size);
        for _ in 0..segment_size {
            sizes.push(self.reader.read_i16() as isize);
        }

        let reader_pos_start = self.reader.pos;
        let mut positions = Vec::with_capacity(segment_size);
        for _ in 0..segment_size {
            positions.push(self.reader.read_u16() as usize);
        }

        for n in 0..segment_size {
            let complete_pos = completers[n] + 1;
            for ch in beginners[n]..complete_pos {
                let symbol;
                if positions[n] == 0 {
                    symbol = ((ch as isize + sizes[n]) & 0xFFFF) as usize;
                } else {
                    let position = (ch - beginners[n]) * 2 + positions[n];
                    let position = reader_pos_start + 2 * n + position;
                    if position >= rim {
                        symbol = 0;
                    } else {
                        let s = self.reader.get_u16_at(position) as usize;
                        if s != 0 {
                            symbol = ((s as isize + sizes[n]) & 0xFFFF) as usize;
                        } else {
                            symbol = 0;
                        }
                    }
                }
                char_symbol_dict.insert(ch, symbol);
                symbol_char_dict.entry(symbol).or_default().push(ch);
            }
        }
    }

    fn parse_hmtx_table(
        &mut self,
        num_h_metrics: usize,
        num_symbols: usize,
        symbol_to_char: &BTreeMap<usize, Vec<usize>>,
    ) {
        let start = self.seek_table("hmtx", 0);
        let raw = self.reader.get_range(start, num_h_metrics * 4);
        let arr = unpack_u16_array(&raw);

        for symbol in 0..num_h_metrics {
            let _aw = arr.get(symbol * 2 + 1).copied().unwrap_or(0);
            // We don't need CharWidths for subsetting; the Go code stores them
            // but we skip that here since we only care about building the subset font.
        }

        // Handle symbols beyond num_h_metrics (same last width) -- also not needed
        // for pure subsetting.
        let _ = (num_symbols, symbol_to_char);
    }

    fn parse_loca_table(&mut self, format: u16, num_symbols: usize) {
        let start = self.seek_table("loca", 0);
        self.symbol_position = Vec::new();

        // NOTE: unpack_u16_array / unpack_u32_array prepend a zero element so
        // the returned slice is 1-indexed — matching Go's `arr[n+1]` access.
        if format == 0 {
            let data = self.reader.get_range(start, (num_symbols * 2) + 2);
            let arr = unpack_u16_array(&data);
            for n in 0..=num_symbols {
                self.symbol_position
                    .push(arr.get(n + 1).copied().unwrap_or(0) * 2);
            }
        } else if format == 1 {
            let data = self.reader.get_range(start, (num_symbols * 4) + 4);
            let arr = unpack_u32_array(&data);
            for n in 0..=num_symbols {
                self.symbol_position
                    .push(arr.get(n + 1).copied().unwrap_or(0));
            }
        }
    }

    fn parse_symbols(&mut self, used_runes: &BTreeMap<usize, usize>) -> ParsedSymbols {
        let mut symbol_collection: BTreeMap<usize, usize> = BTreeMap::new();
        symbol_collection.insert(0, 0);
        let mut char_symbol_pairs: BTreeMap<usize, usize> = BTreeMap::new();

        for ch in used_runes.values() {
            if let Some(&sym) = self.char_symbol_dict.get(ch) {
                symbol_collection.insert(sym, *ch);
                char_symbol_pairs.insert(*ch, sym);
            }
            self.last_rune = self.last_rune.max(*ch);
        }

        let begin = self.table_descs["glyf"].position;

        let symbol_keys: Vec<usize> = symbol_collection.keys().copied().collect();
        let mut symbol_array: BTreeMap<usize, usize> = symbol_keys
            .iter()
            .enumerate()
            .map(|(i, &k)| (k, i))
            .collect();

        let mut rune_symbol_pairs: BTreeMap<usize, usize> = BTreeMap::new();
        for (&runa, &sym) in &char_symbol_pairs {
            if let Some(&new_idx) = symbol_array.get(&sym) {
                rune_symbol_pairs.insert(runa, new_idx);
            }
        }
        self.code_symbol_dict = rune_symbol_pairs.clone();

        // Recursively discover composite glyphs
        let mut keys: Vec<usize> = symbol_collection.keys().copied().collect();
        let mut i = 0;
        while i < keys.len() {
            let old_idx = keys[i];
            self.get_symbols(
                old_idx,
                begin,
                &mut symbol_array,
                &mut symbol_collection,
                &mut keys,
            );
            i += 1;
        }

        (rune_symbol_pairs, symbol_array, symbol_collection, keys)
    }

    fn get_symbols(
        &mut self,
        original_idx: usize,
        begin: usize,
        symbol_set: &mut BTreeMap<usize, usize>,
        symbols_collection: &mut BTreeMap<usize, usize>,
        keys: &mut Vec<usize>,
    ) {
        if original_idx + 1 >= self.symbol_position.len() {
            return;
        }
        let sym_pos = self.symbol_position[original_idx];
        let sym_size = self.symbol_position[original_idx + 1] - sym_pos;
        if sym_size == 0 {
            return;
        }
        self.reader.seek_abs(begin + sym_pos);
        let line_count = self.reader.read_i16();

        if line_count < 0 {
            self.reader.skip(8);
            let mut flags = SYMBOL_CONTINUE;
            while (flags & SYMBOL_CONTINUE) != 0 {
                flags = self.reader.read_u16();
                let symbol_index = self.reader.read_u16() as usize;
                let is_new = !symbol_set.contains_key(&symbol_index);
                if is_new {
                    symbol_set.insert(symbol_index, symbols_collection.len());
                    symbols_collection.insert(symbol_index, 1);
                    keys.push(symbol_index);
                }
                // Only recurse for newly-discovered symbols (prevents cycles).
                // Already-seen symbols are pushed to `keys` so they get processed
                // by the iterative caller loop.
                if is_new {
                    let old_pos = self.reader.pos;
                    self.get_symbols(symbol_index, begin, symbol_set, symbols_collection, keys);
                    self.reader.seek_abs(old_pos);
                }

                if (flags & SYMBOL_WORDS) != 0 {
                    self.reader.skip(4);
                } else {
                    self.reader.skip(2);
                }
                if (flags & SYMBOL_SCALE) != 0 {
                    self.reader.skip(2);
                } else if (flags & SYMBOL_ALL_SCALE) != 0 {
                    self.reader.skip(4);
                } else if (flags & SYMBOL_2X2) != 0 {
                    self.reader.skip(8);
                }
            }
        }
    }

    fn generate_cmap_table(
        &self,
        cid_symbol_pairs: &BTreeMap<usize, usize>,
        _num_symbols: usize,
    ) -> Vec<u8> {
        let mut cid_array: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut cid_id = 0usize;
        let mut prev_cid: isize = -2;
        let mut prev_symbol: isize = -1;

        for (&cid, &sym) in cid_symbol_pairs {
            if cid as isize == prev_cid + 1 && sym as isize == prev_symbol + 1 {
                cid_array.entry(cid_id).or_default().push(sym);
            } else {
                cid_id = cid;
                cid_array.insert(cid_id, vec![sym]);
            }
            prev_cid = cid as isize;
            prev_symbol = sym as isize;
        }

        let cid_array_keys: Vec<usize> = cid_array.keys().copied().collect();
        let seg_count = cid_array.len() + 1;

        let mut search_range = 1usize;
        let mut entry_selector = 0usize;
        while search_range * 2 <= seg_count {
            search_range *= 2;
            entry_selector += 1;
        }
        search_range *= 2;
        let range_shift = seg_count * 2 - search_range;

        let mut data: Vec<i32> = vec![0, 1, 3, 1, 0, 12, 4];
        let mut cmap: Vec<i32> = vec![
            (seg_count * 2) as i32,
            search_range as i32,
            entry_selector as i32,
            range_shift as i32,
        ];

        // endCode
        for &start in &cid_array_keys {
            let end_code = start + (cid_array[&start].len() - 1);
            cmap.push(end_code as i32);
        }
        cmap.push(0xFFFF);
        cmap.push(0); // reservedPad

        // startCode
        for &k in &cid_array_keys {
            cmap.push(k as i32);
        }
        cmap.push(0xFFFF_i32);

        // idDelta
        for &cid_key in &cid_array_keys {
            let id_delta = -(cid_key as i32 - cid_array[&cid_key][0] as i32);
            cmap.push(id_delta);
        }
        cmap.push(1);

        // idRangeOffset
        cmap.resize(cmap.len() + cid_array.len() + 1, 0);

        // glyphIdArray
        for &start in &cid_array_keys {
            for &v in &cid_array[&start] {
                cmap.push(v as i32);
            }
        }
        cmap.push(0);

        let length = (3 + cmap.len()) * 2;
        data.push(length as i32);
        data.push(0);
        data.extend_from_slice(&cmap);

        let mut result = Vec::new();
        for &val in &data {
            result.extend_from_slice(&(val as u16).to_be_bytes());
        }
        result
    }

    fn get_metrics(&mut self, metric_count: usize, gid: usize) -> Vec<u8> {
        let start = self.seek_table("hmtx", 0);
        if gid < metric_count {
            self.reader.seek_abs(start + gid * 4);
            self.reader.read(4).to_vec()
        } else {
            self.reader.seek_abs(start + (metric_count - 1) * 4);
            let mut metrics = self.reader.read(2).to_vec();
            self.reader.seek_abs(start + metric_count * 2 + gid * 2);
            metrics.extend_from_slice(self.reader.read(2));
            metrics
        }
    }

    fn generate_checksum(data: &[u8]) -> [u16; 2] {
        let mut padded;
        let data = if !data.len().is_multiple_of(4) {
            padded = data.to_vec();
            while padded.len() % 4 != 0 {
                padded.push(0);
            }
            &padded
        } else {
            data
        };

        let mut answer = [0u32; 2];
        for i in (0..data.len()).step_by(4) {
            answer[0] += ((data[i] as u32) << 8) + data[i + 1] as u32;
            answer[1] += ((data[i + 2] as u32) << 8) + data[i + 3] as u32;
            answer[0] += answer[1] >> 16;
            answer[1] &= 0xFFFF;
            answer[0] &= 0xFFFF;
        }
        [answer[0] as u16, answer[1] as u16]
    }

    fn calc_int32(x: &mut [u16; 2], y: &[u16; 2]) -> [u16; 2] {
        let mut xw = [x[0] as u32, x[1] as u32];
        let yw = [y[0] as u32, y[1] as u32];

        if yw[1] > xw[1] {
            xw[1] += 1 << 16;
            xw[0] += 1;
        }
        let mut answer1 = xw[1] - yw[1];
        if yw[0] > xw[0] {
            xw[0] += 1 << 16;
        }
        let mut answer0 = xw[0] - yw[0];
        answer0 &= 0xFFFF;
        answer1 &= 0xFFFF; // not strictly needed but safe
        [answer0 as u16, answer1 as u16]
    }

    fn splice(stream: &[u8], offset: usize, value: &[u8]) -> Vec<u8> {
        let mut result = stream.to_vec();
        result[offset..offset + value.len()].copy_from_slice(value);
        result
    }

    fn insert_u16(stream: &[u8], offset: usize, value: u16) -> Vec<u8> {
        Self::splice(stream, offset, &value.to_be_bytes())
    }

    fn assemble_tables(&self) -> Vec<u8> {
        let tables_count = self.out_tables.len();
        let mut find_size = 1usize;
        let mut writer = 0usize;
        while find_size * 2 <= tables_count {
            find_size *= 2;
            writer += 1;
        }
        find_size *= 16;
        let r_offset = tables_count * 16 - find_size;

        // Pack header: version=0x00010000, numTables, searchRange, entrySelector, rangeShift
        let mut answer = Vec::new();
        answer.extend_from_slice(&0x00010000u32.to_be_bytes());
        answer.extend_from_slice(&(tables_count as u16).to_be_bytes());
        answer.extend_from_slice(&(find_size as u16).to_be_bytes());
        answer.extend_from_slice(&(writer as u16).to_be_bytes());
        answer.extend_from_slice(&(r_offset as u16).to_be_bytes());

        let table_names: Vec<&String> = self.out_tables.keys().collect();
        let mut offset = 12 + tables_count * 16;
        let mut begin = 0usize;

        for name in &table_names {
            if name.as_str() == "head" {
                begin = offset;
            }
            answer.extend_from_slice(name.as_bytes());
            let checksum = Self::generate_checksum(&self.out_tables[name.as_str()]);
            answer.extend_from_slice(&checksum[0].to_be_bytes());
            answer.extend_from_slice(&checksum[1].to_be_bytes());
            answer.extend_from_slice(&(offset as u32).to_be_bytes());
            answer.extend_from_slice(&(self.out_tables[name.as_str()].len() as u32).to_be_bytes());
            let padded_length = (self.out_tables[name.as_str()].len() + 3) & !3;
            offset += padded_length;
        }

        for name in &table_names {
            let mut data = self.out_tables[name.as_str()].clone();
            data.extend_from_slice(&[0, 0, 0]);
            let truncated_len = data.len() & !3;
            answer.extend_from_slice(&data[..truncated_len]);
        }

        // Fix head table checksum
        let checksum = Self::generate_checksum(&answer);
        let adj = Self::calc_int32(&mut [0xB1B0, 0xAFBA], &checksum);
        let adj_bytes = [
            (adj[0] >> 8) as u8,
            (adj[0] & 0xFF) as u8,
            (adj[1] >> 8) as u8,
            (adj[1] & 0xFF) as u8,
        ];

        Self::splice(&answer, begin + 8, &adj_bytes)
    }

    fn generate_cut_font(&mut self, used_runes: &BTreeMap<usize, usize>) -> Option<Vec<u8>> {
        self.reader.seek_abs(0);
        self.symbol_position = Vec::new();
        self.char_symbol_dict = BTreeMap::new();
        self.table_descs = BTreeMap::new();
        self.out_tables = BTreeMap::new();
        self.last_rune = 0;
        self.reader.skip(4);
        self.generate_table_descriptions();

        // Read loca format from head table
        self.seek_table("head", 0);
        self.reader.skip(50);
        let mut loca_format = self.reader.read_u16();

        // Read metrics count from hhea
        self.seek_table("hhea", 0);
        self.reader.skip(34);
        let metrics_count = self.reader.read_u16() as usize;
        let old_metrics = metrics_count;

        // Read numSymbols from maxp
        self.seek_table("maxp", 0);
        self.reader.skip(4);
        let num_symbols = self.reader.read_u16() as usize;

        // Generate cmap
        let symbol_char_dict = self.generate_cmap()?;

        // Parse tables
        self.parse_hmtx_table(metrics_count, num_symbols, &symbol_char_dict);
        self.parse_loca_table(loca_format, num_symbols);

        let (mut cid_symbol_pairs, symbol_array, symbol_collection, symbol_keys) =
            self.parse_symbols(used_runes);

        let new_metrics_count = symbol_collection.len();
        let new_num_symbols = new_metrics_count;

        // Copy tables as-is
        let name_data = self.get_table_data("name");
        self.set_out_table("name", name_data);
        let cvt_data = self.get_table_data("cvt ");
        self.set_out_table("cvt ", cvt_data);
        let fpgm_data = self.get_table_data("fpgm");
        self.set_out_table("fpgm", fpgm_data);
        let prep_data = self.get_table_data("prep");
        self.set_out_table("prep", prep_data);
        let gasp_data = self.get_table_data("gasp");
        self.set_out_table("gasp", gasp_data);

        // Rewrite post table
        if let Some(post_table) = self.get_table_data("post") {
            let mut new_post = vec![0x00, 0x03, 0x00, 0x00];
            if post_table.len() >= 16 {
                new_post.extend_from_slice(&post_table[4..16]);
            }
            new_post.extend_from_slice(&[0; 16]);
            self.set_out_table("post", Some(new_post));
        }

        // Remove glyph 0 from cid pairs
        cid_symbol_pairs.remove(&0);

        // Generate cmap table
        let cmap_data = self.generate_cmap_table(&cid_symbol_pairs, new_num_symbols);
        self.set_out_table("cmap", Some(cmap_data));

        // Build glyf and hmtx data
        let glyf_data = self.get_table_data("glyf")?;
        let mut offsets: Vec<usize> = Vec::new();
        let mut new_glyf: Vec<u8> = Vec::new();
        let mut pos = 0usize;
        let mut hmtx_data: Vec<u8> = Vec::new();

        for &original_idx in &symbol_keys {
            let hm = self.get_metrics(old_metrics, original_idx);
            hmtx_data.extend_from_slice(&hm);

            offsets.push(pos);
            if original_idx + 1 >= self.symbol_position.len() {
                continue;
            }
            let sym_pos = self.symbol_position[original_idx];
            let sym_len = self.symbol_position[original_idx + 1] - sym_pos;
            let mut data = glyf_data[sym_pos..sym_pos + sym_len].to_vec();

            if sym_len > 2 {
                let up = ((data[0] as u16) << 8) | (data[1] as u16);
                if (up & (1 << 15)) != 0 {
                    // Composite glyph - rewrite component indices
                    let mut pos_in_sym = 10usize;
                    let mut flags = SYMBOL_CONTINUE;
                    while (flags & SYMBOL_CONTINUE) != 0 && pos_in_sym + 4 <= data.len() {
                        flags = ((data[pos_in_sym] as u16) << 8) | (data[pos_in_sym + 1] as u16);
                        let _sym_idx =
                            ((data[pos_in_sym + 2] as u16) << 8) | (data[pos_in_sym + 3] as u16);
                        let sym_idx = _sym_idx as usize;
                        if let Some(&new_idx) = symbol_array.get(&sym_idx) {
                            let new_idx_u16 = new_idx as u16;
                            data[pos_in_sym + 2] = (new_idx_u16 >> 8) as u8;
                            data[pos_in_sym + 3] = (new_idx_u16 & 0xFF) as u8;
                        }
                        pos_in_sym += 4;
                        if (flags & SYMBOL_WORDS) != 0 {
                            pos_in_sym += 4;
                        } else {
                            pos_in_sym += 2;
                        }
                        if (flags & SYMBOL_SCALE) != 0 {
                            pos_in_sym += 2;
                        } else if (flags & SYMBOL_ALL_SCALE) != 0 {
                            pos_in_sym += 4;
                        } else if (flags & SYMBOL_2X2) != 0 {
                            pos_in_sym += 8;
                        }
                    }
                }
            }

            new_glyf.extend_from_slice(&data);
            pos += sym_len;
            if !pos.is_multiple_of(4) {
                let padding = 4 - (pos % 4);
                new_glyf.extend(std::iter::repeat_n(0u8, padding));
                pos += padding;
            }
        }

        offsets.push(pos);
        self.set_out_table("glyf", Some(new_glyf));
        self.set_out_table("hmtx", Some(hmtx_data));

        // Build loca table
        let mut loca_data = Vec::new();
        if ((pos + 1) >> 1) > 0xFFFF {
            loca_format = 1;
            for &off in &offsets {
                loca_data.extend_from_slice(&(off as u32).to_be_bytes());
            }
        } else {
            loca_format = 0;
            for &off in &offsets {
                loca_data.extend_from_slice(&((off / 2) as u16).to_be_bytes());
            }
        }
        self.set_out_table("loca", Some(loca_data));

        // Update head table
        if let Some(head_data) = self.get_table_data("head") {
            let head_data = Self::insert_u16(&head_data, 50, loca_format);
            self.set_out_table("head", Some(head_data));
        }

        // Update hhea table
        if let Some(hhea_data) = self.get_table_data("hhea") {
            let hhea_data = Self::insert_u16(&hhea_data, 34, new_metrics_count as u16);
            self.set_out_table("hhea", Some(hhea_data));
        }

        // Update maxp table
        if let Some(maxp) = self.get_table_data("maxp") {
            let maxp = Self::insert_u16(&maxp, 4, new_num_symbols as u16);
            self.set_out_table("maxp", Some(maxp));
        }

        // Copy OS/2 table
        let os2_data = self.get_table_data("OS/2");
        self.set_out_table("OS/2", os2_data);

        Some(self.assemble_tables())
    }
}

fn unpack_u16_array(data: &[u8]) -> Vec<usize> {
    let mut result = vec![0usize]; // 1-indexed like the Go code
    let mut i = 0;
    while i + 1 < data.len() {
        result.push(((data[i] as usize) << 8) | (data[i + 1] as usize));
        i += 2;
    }
    result
}

fn unpack_u32_array(data: &[u8]) -> Vec<usize> {
    let mut result = vec![0usize]; // 1-indexed like the Go code
    let mut i = 0;
    while i + 3 < data.len() {
        result.push(
            ((data[i] as usize) << 24)
                | ((data[i + 1] as usize) << 16)
                | ((data[i + 2] as usize) << 8)
                | (data[i + 3] as usize),
        );
        i += 4;
    }
    result
}

/// Subset a TTF font buffer to include only the glyphs used by `corpus`.
///
/// Returns the subset font as a TTF byte buffer.
pub fn utf8_cut_font(font_buf: &[u8], corpus: &str) -> Option<Vec<u8>> {
    let mut f = Utf8FontFile::new(font_buf.to_vec());
    let mut runes: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, ch) in corpus.chars().enumerate() {
        runes.insert(i, ch as usize);
    }
    f.generate_cut_font(&runes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unpack_u16_array() {
        let data = [0x00, 0x01, 0x00, 0x02];
        let arr = unpack_u16_array(&data);
        assert_eq!(arr, vec![0, 1, 2]);
    }

    #[test]
    fn test_unpack_u32_array() {
        let data = [0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
        let arr = unpack_u32_array(&data);
        assert_eq!(arr, vec![0, 1, 2]);
    }
}
