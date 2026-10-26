use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use blast_radius::parse::{self, Kind};

#[derive(Parser)]
#[command(version, about = "Predict what a code change might break")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List every function in a codebase, with its kind and how many calls it makes.
    Functions { dir: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Functions { dir } => {
            let codebase = parse::parse_dir(&dir)?;
            for f in &codebase.functions {
                let kind = match &f.kind {
                    Kind::Function => "fn".to_string(),
                    Kind::Method => "method".to_string(),
                    Kind::Test => "test".to_string(),
                    Kind::Endpoint { method, route } => format!("{method} {route}"),
                };
                println!(
                    "{:32} {:24} {:>2} calls  {}:{}",
                    f.name,
                    kind,
                    f.calls.len(),
                    f.file.display(),
                    f.line
                );
            }
        }
    }
    Ok(())
}
