use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

mod decompress;
mod reader;
mod ring_buffer;
mod tree;
mod writer;

pub fn decompress_file(src: &Path, dest: &Path) -> Result<()> {
    let mut reader = BufReader::new(File::open(src)?);
    let mut writer = BufWriter::new(File::create(dest)?);

    decompress::decompress(&mut reader, &mut writer)?;

    Ok(())
}
