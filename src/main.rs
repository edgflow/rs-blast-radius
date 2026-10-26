use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use blast_radius::config::Config;
use blast_radius::graph::{Confidence, Graph};
use blast_radius::openai::OpenAi;
use blast_radius::parse::{self, Kind};
use blast_radius::{impact, predict, report};

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
    /// Build the call graph and show the most depended-on functions.
    Graph { dir: PathBuf },
    /// What would be affected by changing a function (static analysis only).
    Impact { dir: PathBuf, function: String },
    /// Predict what a proposed change could break: the blast radius plus an AI review.
    Predict {
        dir: PathBuf,
        function: String,
        /// The change you plan to make, in plain words.
        #[arg(long)]
        change: String,
        /// Write the Markdown report to this file instead of printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
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
        Command::Graph { dir } => {
            let graph = Graph::build(&parse::parse_dir(&dir)?);
            let edges: Vec<_> = graph.callees.iter().flatten().collect();
            let count = |c: Confidence| edges.iter().filter(|e| e.confidence == c).count();
            println!(
                "{} functions, {} calls inside the codebase ({} certain, {} likely, {} possible), {} outside it",
                graph.functions.len(),
                edges.len(),
                count(Confidence::Certain),
                count(Confidence::Likely),
                count(Confidence::Possible),
                graph.external_calls,
            );
            let mut by_reach: Vec<(usize, usize)> = (0..graph.functions.len())
                .map(|i| (impact::blast_radius(&graph, i).affected.len(), i))
                .collect();
            by_reach.sort_by(|a, b| b.cmp(a));
            println!("\nMost depended on:");
            for (reach, i) in by_reach.iter().take(5) {
                println!("  {:3} reach  {}", reach, graph.functions[*i].name);
            }
        }
        Command::Impact { dir, function } => {
            let graph = Graph::build(&parse::parse_dir(&dir)?);
            let target = graph.find(&function).map_err(anyhow::Error::msg)?;
            print!(
                "{}",
                report::impact_text(&graph, &impact::blast_radius(&graph, target))
            );
        }
        Command::Predict {
            dir,
            function,
            change,
            out,
        } => {
            let config = Config::from_env()?;
            let codebase = parse::parse_dir(&dir)?;
            let graph = Graph::build(&codebase);
            let target = graph.find(&function).map_err(anyhow::Error::msg)?;
            let impact = impact::blast_radius(&graph, target);
            let ai = OpenAi::new(&config.openai_api_key, &config.openai_base_url);
            let prediction =
                predict::predict(&ai, &config.model, &codebase, &graph, &impact, &change).await?;
            let markdown = report::prediction_markdown(&graph, &impact, &prediction, &change);
            match out {
                Some(path) => {
                    std::fs::write(&path, markdown)?;
                    println!("report written to {}", path.display());
                }
                None => print!("{markdown}"),
            }
        }
    }
    Ok(())
}
