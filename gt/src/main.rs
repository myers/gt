use clap::{Parser, Subcommand};

mod config;
mod issues;

#[derive(Parser)]
#[command(name = "gt", about = "Gitea CLI", version)]
struct App {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage issues
    Issue(issues::IssueCommand),
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let app = App::parse();

    match app.command {
        Command::Issue(cmd) => cmd.run().await?,
    }

    Ok(())
}
