mod huffman;
mod raw;

use crate::reader::Reader;
use crate::writer::Writer;
use crate::DecompressOptions;
use anyhow::{bail, Context, Result};
use encoding_rs::mem::decode_latin1;
use std::convert::TryInto;
use std::io::{Read, Write};

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

struct GzipFlags(u8);

impl GzipFlags {
    #[allow(unused)]
    fn is_text(&self) -> bool {
        (self.0 & 0x01) > 0
    }

    fn has_crc(&self) -> bool {
        (self.0 & 0x02) > 0
    }

    fn has_extra(&self) -> bool {
        (self.0 & 0x04) > 0
    }

    fn has_name(&self) -> bool {
        (self.0 & 0x08) > 0
    }

    fn has_comment(&self) -> bool {
        (self.0 & 0x10) > 0
    }
}

pub fn decompress<R, W>(reader: &mut R, writer: &mut W, opts: &DecompressOptions) -> Result<()>
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

    let flags = GzipFlags(reader.next_byte()?);

    let mtime_bytes = [
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
        reader.next_byte()?,
    ];
    let mtime = u32::from_le_bytes(mtime_bytes);
    let extra_flag = reader.next_byte()?;
    let os = reader.next_byte()?;

    if flags.has_extra() {
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

    let original_name = if flags.has_name() {
        let mut buf = vec![];
        loop {
            let byte = reader.next_byte()?;
            if byte == 0 {
                break;
            } else {
                buf.push(byte);
            }
        }

        let name = decode_latin1(&buf[..]);
        Some(name.into_owned())
    } else {
        None
    };

    let comment = if flags.has_comment() {
        let mut buf = vec![];
        loop {
            let byte = reader.next_byte()?;
            if byte == 0 {
                break;
            } else {
                buf.push(byte);
            }
        }

        let comment = decode_latin1(&buf[..]);
        Some(comment.into_owned())
    } else {
        None
    };

    // TODO: check crc16
    let header_crc16 = if flags.has_crc() {
        let bytes = [reader.next_byte()?, reader.next_byte()?];
        let crc = u16::from_le_bytes(bytes);
        Some(crc)
    } else {
        None
    };

    if opts.show_header {
        let os = match os {
            0 => "FAT filesystem",
            1 => "Amiga",
            2 => "VMS",
            3 => "Unix",
            4 => "VM/CMS",
            5 => "Atari TOS",
            6 => "HPFS filesystem",
            7 => "Macintosh",
            8 => "Z-System",
            9 => "CP/M",
            10 => "TOPS-20",
            11 => "NTFS filesystem",
            12 => "QDOS",
            13 => "Acorn RISCOS",
            255 => "unknown",
            _ => "unknown (undefined value)",
        };

        eprintln!(
            r"magic number      : {:#x} {:#x}
compression method: {:#04x}
flags             : {:#04x}
         FTEXT    : {}
         FHCRC    : {}
         FEXTRA   : {}
         FNAME    : {}
         FCOMMENT : {}
modification time : {}
extra flags       : {:#04x}
os                : {}
original file name: {}
comment           : {}
header CRC        : {}",
            ids[0],
            ids[1],
            cm,
            flags.0,
            flags.is_text(),
            flags.has_crc(),
            flags.has_extra(),
            flags.has_name(),
            flags.has_comment(),
            mtime,
            extra_flag,
            os,
            original_name.unwrap_or_else(|| "(not set)".into()),
            comment.unwrap_or_else(|| "(not set)".into()),
            header_crc16
                .map(|n| format!("{:#06x}", n))
                .unwrap_or_else(|| "(not set)".into()),
        );
    }

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

    if total_bytes & 0xffffffff != data_length.try_into()? {
        bail!(
            "input size differs from actual size; input size = {:#010x}, actual size (modulo 2^32) = {:#010x}",
            data_length,
            total_bytes & 0xffffffff,
        );
    }

    writer.flush()?;

    Ok(())
}
