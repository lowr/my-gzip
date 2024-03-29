mod decompress;
mod reader;
mod ring_buffer;
mod tree;
mod writer;

use anyhow::Result;
use std::fs::File;
use std::io::{sink, BufReader, BufWriter};
use std::path::Path;

#[derive(Debug)]
pub struct DecompressOptions {
    pub show_header: bool,
    pub no_emit: bool,
}

/// decompresses gzip file at `src` into `dest`
pub fn decompress_file(src: &Path, dest: Option<&Path>, opts: DecompressOptions) -> Result<()> {
    let mut reader = BufReader::new(File::open(src)?);

    if opts.no_emit {
        let mut writer = sink();
        decompress::decompress(&mut reader, &mut writer, &opts)?;
    } else {
        // `dest` is guaranteed to be Some by clap
        debug_assert!(dest.is_some());
        let mut writer = BufWriter::new(File::create(dest.unwrap())?);
        decompress::decompress(&mut reader, &mut writer, &opts)?;
    }

    Ok(())
}
