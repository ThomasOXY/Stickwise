use crate::error::{Bytes, Error, Result};

/// Offset of the row heap inside a page.
pub const HEAP_START: usize = 0x28;
/// Bytes taken by one full group of 16 row offsets plus its two flag words.
pub(crate) const GROUP_BYTES: usize = 0x24;

/// Page 0 of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub page_len: u32,
    pub next_unused_page: u32,
    pub unknown: u32,
    pub sequence: u32,
    pub tables: Vec<TablePointer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TablePointer {
    pub ty: u32,
    pub empty_candidate: u32,
    pub first_page: u32,
    pub last_page: u32,
}

impl Header {
    pub fn parse(buf: &[u8]) -> Result<Self> {
        let b = Bytes(buf);
        let page_len = b.u32le(0x04)?;
        if !(512..=65536).contains(&page_len) || !page_len.is_power_of_two() {
            return Err(Error::Invalid(format!("implausible page size {page_len}")));
        }
        let num_tables = b.u32le(0x08)?;
        if num_tables > 64 {
            return Err(Error::Invalid(format!(
                "implausible table count {num_tables}"
            )));
        }
        let tables = (0..num_tables as usize)
            .map(|i| {
                let o = 0x1c + 16 * i;
                Ok(TablePointer {
                    ty: b.u32le(o)?,
                    empty_candidate: b.u32le(o + 4)?,
                    first_page: b.u32le(o + 8)?,
                    last_page: b.u32le(o + 12)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Header {
            page_len,
            next_unused_page: b.u32le(0x0c)?,
            unknown: b.u32le(0x10)?,
            sequence: b.u32le(0x14)?,
            tables,
        })
    }

    pub fn table(&self, ty: u32) -> Option<&TablePointer> {
        self.tables.iter().find(|t| t.ty == ty)
    }
}

/// The 0x28-byte header at the start of every table page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageHeader {
    pub index: u32,
    pub ty: u32,
    pub next: u32,
    pub sequence: u32,
    pub unknown14: u32,
    /// Row offsets ever allocated on this page, including deleted rows.
    pub num_row_offsets: u16,
    /// Rows currently present.
    pub num_rows: u16,
    pub flags: u8,
    pub free_size: u16,
    pub used_size: u16,
    pub transaction_row_count: u16,
    pub transaction_row_index: u16,
    pub unknown24: u16,
    pub unknown26: u16,
}

impl PageHeader {
    pub fn parse(page: &[u8]) -> Result<Self> {
        let b = Bytes(page);
        // 13 bits of row offsets and 11 bits of row count packed little-endian.
        let packed =
            u32::from(b.u8(0x18)?) | u32::from(b.u8(0x19)?) << 8 | u32::from(b.u8(0x1a)?) << 16;
        Ok(PageHeader {
            index: b.u32le(0x04)?,
            ty: b.u32le(0x08)?,
            next: b.u32le(0x0c)?,
            sequence: b.u32le(0x10)?,
            unknown14: b.u32le(0x14)?,
            num_row_offsets: (packed & 0x1fff) as u16,
            num_rows: (packed >> 13) as u16,
            flags: b.u8(0x1b)?,
            free_size: b.u16le(0x1c)?,
            used_size: b.u16le(0x1e)?,
            transaction_row_count: b.u16le(0x20)?,
            transaction_row_index: b.u16le(0x22)?,
            unknown24: b.u16le(0x24)?,
            unknown26: b.u16le(0x26)?,
        })
    }

    /// Index pages (flag 0x40) carry no rows.
    pub fn is_data(&self) -> bool {
        self.flags & 0x40 == 0
    }

    /// Heap offsets of the rows present on this page, in directory order.
    pub fn row_offsets(&self, page: &[u8]) -> Result<Vec<u16>> {
        let b = Bytes(page);
        let len = page.len();
        let mut out = Vec::with_capacity(self.num_rows as usize);
        for i in 0..self.num_row_offsets as usize {
            let base = len
                .checked_sub((i / 16) * GROUP_BYTES)
                .filter(|&base| base >= HEAP_START + GROUP_BYTES)
                .ok_or_else(|| Error::Invalid("row directory overflows page".into()))?;
            let present = b.u16le(base - 4)?;
            if present >> (i % 16) & 1 == 1 {
                out.push(b.u16le(base - 6 - 2 * (i % 16))?);
            }
        }
        Ok(out)
    }
}
