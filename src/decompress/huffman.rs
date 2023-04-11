use crate::reader::Reader;
use crate::tree::{BinaryTrie, NodeType, TreeKey};
use crate::writer::Writer;
use anyhow::{bail, ensure, Context, Result};
use std::convert::TryInto;
use std::io::{Read, Write};

fn read_number_le<R>(reader: &mut Reader<R>, bits: u8) -> Result<usize>
where
    R: Read,
{
    debug_assert!(std::mem::size_of::<usize>() * 8 >= bits.into());

    let mut ret = 0;
    // read from lsb to msb
    for i in 0..bits {
        if reader.next_bit()? {
            ret |= 1 << i;
        }
    }

    Ok(ret)
}

fn build_tree(lengths: &[u8]) -> Result<BinaryTrie> {
    // as per spec, maximum number of bits should be less than 16.
    const BITS_UPPER_BOUND: usize = 16;
    let max_bits = *lengths
        .iter()
        .max()
        .context("cannot build tree from empty slice")?;
    let max_bits = max_bits.into();
    debug_assert!(max_bits < BITS_UPPER_BOUND);

    let mut counts = [0usize; BITS_UPPER_BOUND];
    for &l in lengths {
        let index: usize = l.into();
        counts[index] += 1;
    }

    let mut next_code = [0usize; BITS_UPPER_BOUND];
    for bits in 2..=max_bits {
        next_code[bits] = (next_code[bits - 1] + counts[bits - 1]) << 1;
    }

    let mut tree = BinaryTrie::new();

    for (n, &length) in lengths.iter().enumerate() {
        if length == 0 {
            continue;
        }

        let length: usize = length.into();
        let code = next_code[length];
        // TODO: this assertion may be done while building `next_code`
        if code >= (1 << length) {
            bail!(
                "code for {} expected to be {} bits, turned out to be {:#b}",
                n,
                length,
                code
            );
        }
        next_code[length] += 1;
        tree.add(
            TreeKey(code.try_into().unwrap(), length),
            n.try_into().unwrap(),
        )?;
    }

    Ok(tree)
}

fn read_next_code<R>(reader: &mut Reader<R>, tree: &BinaryTrie) -> Result<u64>
where
    R: Read,
{
    let mut cursor = tree.cursor();

    loop {
        let bit = reader.next_bit()?;
        if let NodeType::LeafNode(v) = cursor.follow(bit)? {
            return Ok(v);
        }
    }
}

fn read_code_lengths<R, W>(
    reader: &mut Reader<R>,
    writer: &mut W,
    tree: &BinaryTrie,
    count: usize,
) -> Result<()>
where
    R: Read,
    W: Write,
{
    let mut remain = count;
    let mut prev = None;

    while remain > 0 {
        let c = read_next_code(reader, tree)?;

        match c {
            0..=15 => {
                // literal; represents the value itself
                let b = c.try_into().unwrap();
                writer.write_all(&[b])?;
                prev = Some(b);
                remain -= 1;
            }
            16 => {
                // copy the previous code length 3 - 6 times
                let repeat_length = read_number_le(reader, 2)? + 3;

                ensure!(
                    repeat_length <= remain,
                    "too long; repeat_length = {}, remain = {}",
                    repeat_length,
                    remain
                );

                if let Some(b) = prev {
                    let buf = &[b; 6][..repeat_length];
                    writer.write_all(buf)?;
                    remain -= repeat_length;
                } else {
                    bail!("no previous value");
                }
            }
            17..=18 => {
                let (length_bits, addend) = if c == 17 { (3, 3) } else { (7, 11) };
                let repeat_length = read_number_le(reader, length_bits)? + addend;

                ensure!(
                    repeat_length <= remain,
                    "too long; repeat_length = {}, remain = {}",
                    repeat_length,
                    remain
                );

                for _ in 0..repeat_length {
                    writer.write_all(&[0])?;
                }

                prev = Some(0);
                remain -= repeat_length;
            }
            // shouldn't be a problem when `tree` is constructed by `build()`
            _ => unreachable!(),
        }
    }

    Ok(())
}

pub fn read_compressed_data<R, W>(
    reader: &mut Reader<R>,
    writer: &mut Writer<W>,
    lit_tree: &BinaryTrie,
    dist_tree: &BinaryTrie,
) -> Result<usize>
where
    R: Read,
    W: Write,
{
    let mut bytes = 0;

    loop {
        #[rustfmt::skip]
        const LENGTH_INFO: [(u8, usize); 29] = [
            // 257..=264
            (0, 3), (0, 4), (0, 5), (0, 6), (0, 7), (0, 8), (0, 9), (0, 10),
            // 265..=268
            (1, 11), (1, 13), (1, 15), (1, 17),
            // 269..=272
            (2, 19), (2, 23), (2, 27), (2, 31),
            // 273..=276
            (3, 35), (3, 43), (3, 51), (3, 59),
            // 277..=280
            (4, 67), (4, 83), (4, 99), (4, 115),
            // 281..=284
            (5, 131), (5, 163), (5, 195), (5, 227),
            // 285
            (0, 258),
        ];

        #[rustfmt::skip]
        const DIST_INFO: [(u8, usize); 30] = [
            // 0..=3
            (0, 1), (0, 2), (0, 3), (0, 4),
            // 4..=11
            (1, 5), (1, 7), (2, 9), (2, 13), (3, 17), (3, 25), (4, 33), (4, 49),
            // 12..=17
            (5, 65), (5, 97), (6, 129), (6, 193), (7, 257), (7, 385),
            // 18..=23
            (8, 513), (8, 769), (9, 1025), (9, 1537), (10, 2049), (10, 3073),
            // 24..=29
            (11, 4097), (11, 6145), (12, 8193), (12, 12289), (13, 16385), (13, 24577),
        ];

        let c = read_next_code(reader, lit_tree)?;
        match c {
            0..=255 => {
                // literal; represents the value itself
                let b = c.try_into().unwrap();
                writer.push(b)?;
                bytes += 1;
            }
            256 => break,
            257..=285 => {
                let index: usize = (c - 257).try_into().unwrap();
                let (length_bits, addend) = LENGTH_INFO[index];
                let length = read_number_le(reader, length_bits)? + addend;

                let dist_code = read_next_code(reader, dist_tree)?;
                let index: usize = dist_code.try_into().unwrap();
                let (length_bits, addend) = DIST_INFO[index];
                let dist = read_number_le(reader, length_bits)? + addend;

                let len = writer.copy_within(dist, length)?;
                bytes += len;
            }
            _ => unreachable!(),
        }
    }

    Ok(bytes)
}

pub fn decompress_dynamic<R, W>(reader: &mut Reader<R>, writer: &mut Writer<W>) -> Result<usize>
where
    R: Read,
    W: Write,
{
    let hlit = read_number_le(reader, 5).context("unable to read HLIT")? + 257;
    let hdist = read_number_le(reader, 5).context("unable to read HDIST")? + 1;
    let hclen = read_number_le(reader, 4).context("unable to read HCLEN")? + 4;

    const ALPHABET_ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    let mut lengths = [0; 19];
    for &i in ALPHABET_ORDER.iter().take(hclen) {
        lengths[i] = read_number_le(reader, 3)?.try_into().unwrap();
    }

    let code_tree = build_tree(&lengths)?;

    let mut code_lengths = Vec::with_capacity(hlit + hdist);
    read_code_lengths(reader, &mut code_lengths, &code_tree, hlit + hdist)?;
    let (lit, dist) = code_lengths.split_at(hlit);

    let lit_tree = build_tree(lit)?;
    let dist_tree = build_tree(dist)?;

    let bytes = read_compressed_data(reader, writer, &lit_tree, &dist_tree)?;

    Ok(bytes)
}

const fn build_lit_lengths() -> [u8; 288] {
    let mut lit = [8; 288];

    let mut i = 144;
    while i < 256 {
        lit[i] = 9;
        i += 1;
    }
    while i < 280 {
        lit[i] = 7;
        i += 1;
    }

    lit
}

const LIT_LENGTHS: [u8; 288] = build_lit_lengths();
const DIST_LENGTHS: [u8; 32] = [5; 32];

thread_local!(
    // guaranteed to be infallible
    static LIT_TREE: BinaryTrie = build_tree(&LIT_LENGTHS).unwrap();
    static DIST_TREE: BinaryTrie = build_tree(&DIST_LENGTHS).unwrap();
);

pub fn decompress_fixed<R, W>(reader: &mut Reader<R>, writer: &mut Writer<W>) -> Result<usize>
where
    R: Read,
    W: Write,
{
    let bytes = LIT_TREE.with(|lit_tree| {
        DIST_TREE.with(|dist_tree| read_compressed_data(reader, writer, lit_tree, dist_tree))
    })?;

    Ok(bytes)
}
