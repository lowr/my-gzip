use anyhow::Result;
use clap::Clap;
use std::ffi::OsString;
use std::path::Path;

#[derive(Clap)]
struct Opts {
    src: OsString,
    dest: OsString,
    #[clap(long)]
    show_header: bool,
    #[clap(long)]
    no_emit: bool,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let src = Path::new(&opts.src);
    let dest = Path::new(&opts.dest);

    let opts = my_gzip::DecompressOptions {
        show_header: opts.show_header,
        no_emit: opts.no_emit,
    };

    my_gzip::decompress_file(src, dest, opts)?;

    Ok(())
}
