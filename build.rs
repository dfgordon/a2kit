use clap::ValueEnum;
use clap_complete::{generate_to, Shell};
use std::env;
use std::io::Error;

include!("src/cli.rs");

fn main() -> Result<(), Error> {
    let outdir = match env::var_os("CARGO_MANIFEST_DIR") {
        None => return Ok(()),
        Some(root) => std::path::Path::new(&root).join("completions"),
    };

    let mut cmd = build_cli();

    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "a2kit", &outdir)?;
    }

    Ok(())
}