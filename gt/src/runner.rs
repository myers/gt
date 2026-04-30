use clap::{Args, Subcommand};
use eyre::Result;

use crate::repo;

#[derive(Args)]
pub struct RunnerCommand {
    #[command(flatten)]
    repo: repo::RepoArgs,

    #[command(subcommand)]
    action: RunnerAction,
}

#[derive(Subcommand)]
enum RunnerAction {
    /// List runners
    List,
}

impl RunnerCommand {
    pub async fn run(&self) -> Result<()> {
        match &self.action {
            RunnerAction::List => {
                eyre::bail!("not implemented yet");
            }
        }
    }
}
