use std::{env, path::PathBuf};

use clap::CommandFactory;

#[allow(dead_code)]
#[path = "src/cli.rs"]
mod cli;

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=src/cli.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    clap_mangen::generate_to(cli::CliArgs::command(), out_dir)
}
