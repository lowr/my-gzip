use anyhow::Result;
use clap::Clap;
use std::path::PathBuf;

#[derive(Clap)]
struct Opts {
    #[clap(parse(from_os_str))]
    /// Path for source file.
    src: PathBuf,
    #[clap(parse(from_os_str), required_unless_present("no-emit"))]
    /// Path for destination file. Required unless `--no-emit` is specified.
    dest: Option<PathBuf>,
    #[clap(long)]
    /// Prints header to stderr.
    show_header: bool,
    #[clap(long)]
    /// Do not emit decompressed content. <dest> would be ignored if specified.
    no_emit: bool,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let src = opts.src.as_path();
    let dest = opts.dest.as_deref();

    let opts = my_gzip::DecompressOptions {
        show_header: opts.show_header,
        no_emit: opts.no_emit,
    };

    my_gzip::decompress_file(src, dest, opts)?;

    Ok(())
}
