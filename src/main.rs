use anyhow::Result;
use clap::Clap;
use std::ffi::OsString;
use std::path::Path;

#[derive(Clap)]
struct Opts {
    src: OsString,
    dest: OsString,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let src = Path::new(&opts.src);
    let dest = Path::new(&opts.dest);

    my_gzip::decompress_file(src, dest)?;

    Ok(())
}
