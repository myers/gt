use clap::{Args, CommandFactory, Parser, Subcommand};

mod api;
mod auth;
mod browse;
mod config;
mod config_cmd;
mod issues;
mod label;
mod milestone;
mod org;
mod project;
mod pulls;
mod release;
mod repo;
mod repo_cmd;
mod run;

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
    /// Manage pull requests
    Pr(pulls::PrCommand),
    /// Manage repositories
    Repo(repo_cmd::RepoCommand),
    /// Manage labels
    Label(label::LabelCommand),
    /// Manage milestones
    Milestone(milestone::MilestoneCommand),
    /// Manage releases
    Release(release::ReleaseCommand),
    /// Manage projects
    Project(project::ProjectCommand),
    /// Manage Actions workflow runs
    Run(run::RunCommand),
    /// Manage organizations
    Org(org::OrgCommand),
    /// Authentication commands
    Auth(auth::AuthCommand),
    /// Manage configuration
    Config(config_cmd::ConfigCommand),
    /// Open in browser
    Browse(browse::BrowseCommand),
    /// Make an authenticated API request
    Api(api::ApiCommand),
    /// Generate shell completions
    Completion(CompletionArgs),
}

#[derive(Args)]
struct CompletionArgs {
    /// Shell to generate for (bash, zsh, fish, powershell, elvish)
    shell: clap_complete::Shell,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let app = App::parse();

    match app.command {
        Command::Issue(cmd) => cmd.run().await?,
        Command::Pr(cmd) => cmd.run().await?,
        Command::Repo(cmd) => cmd.run().await?,
        Command::Label(cmd) => cmd.run().await?,
        Command::Milestone(cmd) => cmd.run().await?,
        Command::Release(cmd) => cmd.run().await?,
        Command::Project(cmd) => cmd.run().await?,
        Command::Run(cmd) => cmd.run().await?,
        Command::Org(cmd) => cmd.run().await?,
        Command::Auth(cmd) => cmd.run().await?,
        Command::Config(cmd) => cmd.run().await?,
        Command::Browse(cmd) => cmd.run().await?,
        Command::Api(cmd) => cmd.run().await?,
        Command::Completion(args) => {
            clap_complete::generate(
                args.shell,
                &mut App::command(),
                "gt",
                &mut std::io::stdout(),
            );
        }
    }

    Ok(())
}
