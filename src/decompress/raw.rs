use crate::reader::Reader;
use crate::writer::Writer;
use anyhow::{bail, Result};
use std::io::{Read, Write};

pub fn decompress<R, W>(reader: &mut Reader<R>, writer: &mut Writer<W>) -> Result<usize>
where
    R: Read,
    W: Write,
{
    // TODO: check unread bits if any
    let len = u16::from_le_bytes([reader.next_byte()?, reader.next_byte()?]);
    let nlen = u16::from_le_bytes([reader.next_byte()?, reader.next_byte()?]);

    // `nlen` must be one's complement of `len` i.e. bit-wise inversion of `len`
    if len != !nlen {
        bail!(
            "inconsistency between LEN and NLEN bytes: LEN = {:#010b}, NLEN = {:#010b}",
            len,
            nlen
        );
    }

    let len = len.into();

    writer.copy_from(reader, len)?;

    Ok(len)
}
