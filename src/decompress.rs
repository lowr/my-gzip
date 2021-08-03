use crate::reader::Reader;
use crate::writer::Writer;
use anyhow::{bail, Context, Result};
use encoding_rs::mem::decode_latin1;
use std::convert::TryInto;
use std::io::{Read, Write};

mod huffman;
mod raw;

// returns (bytes decompressed, whether this is the final block)
pub fn decompress_block<R, W>(
    reader: &mut Reader<R>,
    writer: &mut Writer<W>,
) -> Result<(usize, bool)>
where
    R: Read,
    W: Write,
{
    let final_block = reader.next_bit()?;
    let bytes = match [reader.next_bit()?, reader.next_bit()?] {
        [false, false] => raw::decompress(reader, writer)?,
        [true, false] => huffman::decompress_fixed(reader, writer)?,
        [false, true] => huffman::decompress_dynamic(reader, writer)?,
        _ => bail!("block type 11 is reserved"),
    };

    Ok((bytes, final_block))
}

#[derive(Debug)]
struct GzipFlags {
    text: bool,
    crc: bool,
    extra: bool,
    name: bool,
    comment: bool,
}

impl GzipFlags {
    fn new(byte: u8) -> Self {
        Self {
            text: (byte & 0x01) > 0,
            crc: (byte & 0x02) > 0,
            extra: (byte & 0x04) > 0,
            name: (byte & 0x08) > 0,
            comment: (byte & 0x10) > 0,
        }
    }
}

pub fn decompress<R, W>(reader: &mut R, writer: &mut W) -> Result<()>
where
    R: Read,
    W: Write,
{
    let mut reader = Reader::new(reader);
    // maximum distance is 32768
    let mut writer = Writer::new(writer, 32768);

    // header verification

    // magic number
    let mut ids = [0; 2];
    reader
        .copy_to(&mut &mut ids[..], 2)
        .context("failed to read magic numbers")?;
    if ids[0] != 0x1f || ids[1] != 0x8b {
        bail!(
            "wrong magic number; ID1 = {:#x} (expected 0x1f), ID2 = {:#x} (expected 0x8b)",
            ids[0],
            ids[1],
        );
    }

    // compression method
    let cm = reader.next_byte()?;
    if cm != 8 {
        bail!(
            "wrong compression method detected; CM = {:#x} (expected 0x08)",
            cm,
        );
    }

    let flags = GzipFlags::new(reader.next_byte()?);

    let mtime_bytes = [
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
    ];
    let _mtime = u32::from_le_bytes(mtime_bytes);
    let _extra_flag = reader.next_byte()?;
    let _os = reader.next_byte()?;

    if flags.extra {
        let length_bytes = [reader.next_byte()?, reader.next_byte()?];
        let length = u16::from_le_bytes(length_bytes).into();
        // TODO: handle extra fields properly
        let consumed = reader.skip(length)?;

        if length != consumed {
            bail!(
                "extra field: failed to read {} bytes; only {} bytes were read",
                length,
                consumed,
            );
        }
    }

    if flags.name {
        let mut buf = vec![];
        loop {
            let byte = reader.next_byte()?;
            if byte == 0 {
                break;
            } else {
                buf.push(byte);
            }
        }

        // TODO: handle file name properly
        let _name = decode_latin1(&buf[..]);
        // eprintln!("original file name = {}", _name);
    }

    if flags.comment {
        let mut buf = vec![];
        loop {
            let byte = reader.next_byte()?;
            if byte == 0 {
                break;
            } else {
                buf.push(byte);
            }
        }

        // TODO: handle and output comment
        let _comment = decode_latin1(&buf[..]);
        // eprintln!("comment = {}", _comment);
    }

    // TODO: check crc16
    let _header_crc16 = if flags.crc {
        let bytes = [reader.next_byte()?, reader.next_byte()?];
        let crc = u16::from_le_bytes(bytes);
        Some(crc)
    } else {
        None
    };

    // actual decompression
    let mut total_bytes = 0;
    loop {
        let (bytes, final_block) = decompress_block(&mut reader, &mut writer)?;
        total_bytes += bytes;
        if final_block {
            break;
        }
    }

    // TODO: check unread bits if any
    reader.ensure_byte_boundary()?;

    // TODO: check crc32
    let data_crc32_bytes = [
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
    ];
    let _data_crc32 = u32::from_le_bytes(data_crc32_bytes);

    let data_length_bytes = [
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
    ];
    let data_length = u32::from_le_bytes(data_length_bytes);

    if total_bytes & ((1 << 32) - 1) != data_length.try_into()? {
        bail!(
            "input size differs from actual size; input size = {:#x}, actual size (modulo 2^32) = {:#x}",
            data_length,
            total_bytes & ((1 << 32) - 1),
        );
    }

    writer.flush()?;

    Ok(())
}
