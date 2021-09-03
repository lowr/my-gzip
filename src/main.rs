use anyhow::Result;
use clap::Clap;
use std::path::PathBuf;

#[derive(Clap)]
struct Opts {
    #[clap(parse(from_os_str))]
    src: PathBuf,
    #[clap(parse(from_os_str), required_unless_present("no-emit"))]
    dest: Option<PathBuf>,
    #[clap(long)]
    show_header: bool,
    #[clap(long)]
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
