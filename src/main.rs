use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Opts {
    /// Path for source file.
    src: PathBuf,
    #[clap(required_unless_present("no_emit"))]
    /// Path for destination file. Required unless `--no-emit` is specified.
    dest: Option<PathBuf>,
    #[arg(long)]
    /// Prints header to stderr.
    show_header: bool,
    #[arg(long)]
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
