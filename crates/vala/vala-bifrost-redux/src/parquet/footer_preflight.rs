//! Allocation-free structural preflight for Compact-Thrift Parquet footers.

/// Maximum recursive Compact-Thrift container depth.
pub(crate) const MAX_FOOTER_DEPTH: usize = 32;
/// Maximum declared members in any one Compact-Thrift collection.
pub(crate) const MAX_FOOTER_COLLECTION_ITEMS: usize = 4_096;
/// Structural elements one row group's column chunk may declare.
///
/// A `ColumnChunk` contributes its own fields, the `ColumnMetaData` fields, the
/// encodings and schema-path lists, both bound statistics structs, and the page
/// and dictionary offsets. The widest writer-v2 object measured — 1,024 row
/// groups of 64 leaf columns — declares just over thirty-seven members per
/// chunk, so forty-eight admits it with room for the optional members writer-v2
/// does not currently emit.
const MAX_FOOTER_ELEMENTS_PER_COLUMN_CHUNK: usize = 48;
/// Structural elements one footer declares outside its column chunks.
///
/// The schema element list, the writer-v2 key/value metadata, and the
/// file-level fields are all bounded and small next to the chunk metadata, so
/// one flat allowance covers them without a second derivation.
const MAX_FOOTER_FILE_LEVEL_ELEMENTS: usize = 4_096;
/// Maximum aggregate fields and collection members in one footer.
///
/// Derived from the writer-v2 object contract rather than chosen: a footer may
/// describe [`MAX_FILE_COLUMN_CHUNKS`] chunks, each declaring at most
/// [`MAX_FOOTER_ELEMENTS_PER_COLUMN_CHUNK`] members, plus the file-level
/// allowance. A flat ceiling below that refuses objects the writer is required
/// to produce: an assembled hot object at the approximately 512 MiB staging
/// target holds roughly sixteen row groups, and sixteen groups of an ordinary
/// eleven-column table already declare more than four thousand elements.
pub(crate) const MAX_FOOTER_STRUCTURAL_ELEMENTS: usize =
    crate::parquet::memory::MAX_FILE_COLUMN_CHUNKS * MAX_FOOTER_ELEMENTS_PER_COLUMN_CHUNK
        + MAX_FOOTER_FILE_LEVEL_ELEMENTS;
/// Maximum one binary/string value accepted before decoder allocation.
pub(crate) const MAX_FOOTER_STRING_BYTES: usize = 8 * 1024 * 1024;

/// Validates Compact-Thrift declarations without allocating from them.
///
/// # Errors
/// Returns a stable refusal description for truncation, invalid type tags,
/// excessive nesting, oversized strings or collections, aggregate structural
/// pressure, and trailing bytes after the top-level struct.
pub(crate) fn preflight_compact_thrift(bytes: &[u8]) -> Result<(), String> {
    compact_thrift_structural_elements(bytes).map(|_| ())
}

/// Returns the checked structural-element count of one Compact-Thrift footer.
///
/// The decoder proof uses this same allocation-free scan to select the largest
/// accepted real Parquet footer before passing it to the standard decoder.
///
/// # Errors
/// Returns the same bounded refusal as [`preflight_compact_thrift`] for
/// malformed, truncated, oversized, or trailing input.
pub(crate) fn compact_thrift_structural_elements(bytes: &[u8]) -> Result<usize, String> {
    let mut scanner = CompactScanner {
        bytes,
        cursor: 0,
        elements: 0,
    };
    scanner.scan_struct(0)?;
    if scanner.cursor != bytes.len() {
        return Err("Compact-Thrift footer has trailing bytes".to_owned());
    }
    Ok(scanner.elements)
}

/// Cursor and closed structural counters for one encoded footer.
struct CompactScanner<'a> {
    /// Encoded footer bytes already bounded by the 8 MiB allowance.
    bytes: &'a [u8],
    /// Next unread byte.
    cursor: usize,
    /// Aggregate field and collection-member count.
    elements: usize,
}

impl CompactScanner<'_> {
    /// Scans fields until the Compact-Thrift STOP marker.
    fn scan_struct(&mut self, depth: usize) -> Result<(), String> {
        Self::require_depth(depth)?;
        loop {
            let header = self.byte()?;
            let kind = header & 0x0f;
            if kind == 0 {
                return Ok(());
            }
            self.add_elements(1)?;
            if header >> 4 == 0 {
                self.varint()?;
            }
            self.scan_value(kind, depth + 1)?;
        }
    }

    /// Scans one value for a validated Compact-Thrift type tag.
    fn scan_value(&mut self, kind: u8, depth: usize) -> Result<(), String> {
        Self::require_depth(depth)?;
        match kind {
            1 | 2 => Ok(()),
            3 => self.skip(1),
            4..=6 => self.varint().map(|_| ()),
            7 => self.skip(8),
            8 => {
                let length = self.varint_usize()?;
                if length > MAX_FOOTER_STRING_BYTES {
                    return Err("Compact-Thrift footer string exceeds its ceiling".to_owned());
                }
                self.skip(length)
            }
            9 | 10 => self.scan_list(depth),
            11 => self.scan_map(depth),
            12 => self.scan_struct(depth),
            _ => Err(format!("Compact-Thrift footer type tag {kind} is invalid")),
        }
    }

    /// Scans one list/set declaration and every encoded member.
    fn scan_list(&mut self, depth: usize) -> Result<(), String> {
        let header = self.byte()?;
        let mut count = usize::from(header >> 4);
        let kind = header & 0x0f;
        if count == 15 {
            count = self.varint_usize()?;
        }
        Self::require_collection(count)?;
        self.add_elements(count)?;
        for _ in 0..count {
            self.scan_collection_value(kind, depth + 1)?;
        }
        Ok(())
    }

    /// Scans one map declaration and every key/value pair.
    fn scan_map(&mut self, depth: usize) -> Result<(), String> {
        let count = self.varint_usize()?;
        if count == 0 {
            return Ok(());
        }
        Self::require_collection(count)?;
        self.add_elements(
            count
                .checked_mul(2)
                .ok_or_else(|| "Compact-Thrift map count overflows".to_owned())?,
        )?;
        let kinds = self.byte()?;
        let key = kinds >> 4;
        let value = kinds & 0x0f;
        for _ in 0..count {
            self.scan_collection_value(key, depth + 1)?;
            self.scan_collection_value(value, depth + 1)?;
        }
        Ok(())
    }

    /// Scans a collection value where booleans occupy one encoded byte.
    fn scan_collection_value(&mut self, kind: u8, depth: usize) -> Result<(), String> {
        if matches!(kind, 1 | 2) {
            self.skip(1)
        } else {
            self.scan_value(kind, depth)
        }
    }

    /// Reads one bounded unsigned LEB128 integer.
    fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0_u64;
        for shift in (0..70).step_by(7) {
            let byte = self.byte()?;
            if shift == 63 && byte > 1 {
                return Err("Compact-Thrift varint overflows".to_owned());
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("Compact-Thrift varint is unterminated".to_owned())
    }

    /// Reads one varint that must fit the process address space.
    fn varint_usize(&mut self) -> Result<usize, String> {
        usize::try_from(self.varint()?)
            .map_err(|_| "Compact-Thrift length exceeds address space".to_owned())
    }

    /// Returns one byte or a deterministic truncation refusal.
    fn byte(&mut self) -> Result<u8, String> {
        let byte = self
            .bytes
            .get(self.cursor)
            .copied()
            .ok_or_else(|| "Compact-Thrift footer is truncated".to_owned())?;
        self.cursor += 1;
        Ok(byte)
    }

    /// Advances across an already bounded scalar/string payload.
    fn skip(&mut self, bytes: usize) -> Result<(), String> {
        self.cursor = self
            .cursor
            .checked_add(bytes)
            .filter(|cursor| *cursor <= self.bytes.len())
            .ok_or_else(|| "Compact-Thrift footer is truncated".to_owned())?;
        Ok(())
    }

    /// Enforces the recursive decoder-stack ceiling.
    fn require_depth(depth: usize) -> Result<(), String> {
        if depth > MAX_FOOTER_DEPTH {
            return Err("Compact-Thrift footer nesting exceeds its ceiling".to_owned());
        }
        Ok(())
    }

    /// Enforces the per-container declaration ceiling before iteration.
    fn require_collection(count: usize) -> Result<(), String> {
        if count > MAX_FOOTER_COLLECTION_ITEMS {
            return Err("Compact-Thrift footer collection exceeds its ceiling".to_owned());
        }
        Ok(())
    }

    /// Enforces the aggregate structural ceiling with checked arithmetic.
    fn add_elements(&mut self, count: usize) -> Result<(), String> {
        self.elements = self
            .elements
            .checked_add(count)
            .ok_or_else(|| "Compact-Thrift structural count overflows".to_owned())?;
        if self.elements > MAX_FOOTER_STRUCTURAL_ELEMENTS {
            return Err(format!(
                "Compact-Thrift footer structural count exceeds its ceiling: {} declared, {MAX_FOOTER_STRUCTURAL_ELEMENTS} allowed",
                self.elements
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny valid structs pass while a huge list declaration refuses before iteration.
    #[test]
    fn compact_thrift_preflight_refuses_tiny_huge_list_declaration() {
        assert!(preflight_compact_thrift(&[0]).is_ok());
        let huge = [0x19, 0xf5, 0x81, 0x20, 0x00];
        assert!(preflight_compact_thrift(&huge).is_err());
    }

    /// Nested structs stop exactly at the depth boundary and refuse one deeper.
    #[test]
    fn compact_thrift_preflight_enforces_depth_boundary() {
        let mut accepted = vec![0x1c; MAX_FOOTER_DEPTH];
        accepted.extend(std::iter::repeat_n(0x00, MAX_FOOTER_DEPTH + 1));
        assert!(preflight_compact_thrift(&accepted).is_ok());

        let mut refused = vec![0x1c; MAX_FOOTER_DEPTH + 1];
        refused.extend(std::iter::repeat_n(0x00, MAX_FOOTER_DEPTH + 2));
        assert!(preflight_compact_thrift(&refused).is_err());
    }

    /// Encodes one unsigned Compact-Thrift varint for boundary fixtures.
    fn encode_varint(mut value: usize) -> Vec<u8> {
        let mut encoded = Vec::new();
        loop {
            let mut byte = u8::try_from(value & 0x7f).expect("seven bits fit u8");
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            encoded.push(byte);
            if value == 0 {
                return encoded;
            }
        }
    }

    /// Collection and aggregate element ceilings accept their exact maxima only.
    #[test]
    fn compact_thrift_preflight_enforces_count_and_total_boundaries() {
        assert!(CompactScanner::require_collection(MAX_FOOTER_COLLECTION_ITEMS).is_ok());
        assert!(CompactScanner::require_collection(MAX_FOOTER_COLLECTION_ITEMS + 1).is_err());

        // The aggregate ceiling now sits far above the per-collection one, so a
        // footer that reaches it declares hundreds of collections rather than
        // one. The accumulator itself is what the boundary lives on, so drive it
        // directly instead of encoding a multi-megabyte fixture whose first
        // refusal would come from the collection ceiling.
        let mut scanner = CompactScanner {
            bytes: &[],
            cursor: 0,
            elements: 0,
        };
        scanner
            .add_elements(MAX_FOOTER_STRUCTURAL_ELEMENTS)
            .expect("the exact structural maximum is accepted");
        assert!(
            scanner.add_elements(1).is_err(),
            "one element past the ceiling is refused"
        );

        let mut small = vec![0x19, 0xf1];
        small.extend(encode_varint(MAX_FOOTER_COLLECTION_ITEMS));
        small.extend(std::iter::repeat_n(1, MAX_FOOTER_COLLECTION_ITEMS));
        small.push(0);
        assert!(preflight_compact_thrift(&small).is_ok());
        assert_eq!(
            compact_thrift_structural_elements(&small).expect("one full collection"),
            MAX_FOOTER_COLLECTION_ITEMS + 1
        );
    }

    /// Binary declarations accept exactly 8 MiB and refuse one byte more.
    #[test]
    fn compact_thrift_preflight_enforces_string_boundary() {
        let mut accepted = vec![0x18];
        accepted.extend(encode_varint(MAX_FOOTER_STRING_BYTES));
        accepted.resize(accepted.len() + MAX_FOOTER_STRING_BYTES, 0);
        accepted.push(0);
        assert!(preflight_compact_thrift(&accepted).is_ok());

        let mut refused = vec![0x18];
        refused.extend(encode_varint(MAX_FOOTER_STRING_BYTES + 1));
        refused.push(0);
        assert!(preflight_compact_thrift(&refused).is_err());
    }
}
