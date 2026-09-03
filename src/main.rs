use cacheconsistency::{compare_runs, read_run, record_command, snapshot_path};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "cacheconsistency",
    version,
    about = "Audit local and simulated filesystem consistency"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Record {
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    Compare {
        left: PathBuf,
        right: PathBuf,
    },
    VerifyTree {
        path: PathBuf,
    },
}

fn run(cli: Cli) -> Result<(), String> {
    let value = match cli.command {
        Command::Record { command } => serde_json::to_value(
            record_command(
                &command,
                &std::env::current_dir().map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?,
        Command::Compare { left, right } => serde_json::to_value(
            compare_runs(
                &read_run(&left).map_err(|error| error.to_string())?,
                &read_run(&right).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?,
        Command::VerifyTree { path } => {
            serde_json::to_value(snapshot_path(&path, "local").map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("cacheconsistency: {error}");
        std::process::exit(2);
    }
}
