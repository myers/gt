use clap::{Args, CommandFactory, Parser, Subcommand};

mod api;
mod auth;
mod body;
mod browse;
mod config;
mod config_cmd;
mod issues;
mod json;
mod label;
mod milestone;
mod notification;
mod org;
mod paginate;
mod project;
mod prompt;
mod pulls;
mod release;
mod repo;
mod repo_cmd;
mod run;
mod search;
mod secret;
mod variable;

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
    /// Search repos, issues, users
    Search(search::SearchCommand),
    /// Manage repository secrets
    Secret(secret::SecretCommand),
    /// Manage repository variables
    Variable(variable::VariableCommand),
    /// Manage notifications
    Notification(notification::NotificationCommand),
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
#[command(after_long_help = "\
Install completions:

  # bash
  gt completion bash > ~/.local/share/bash-completion/completions/gt

  # zsh (add fpath=(~/.zfunc $fpath) to .zshrc first)
  gt completion zsh > ~/.zfunc/_gt

  # fish
  gt completion fish > ~/.config/fish/completions/gt.fish
")]
struct CompletionArgs {
    /// Shell to generate for (bash, zsh, fish, powershell, elvish)
    shell: clap_complete::Shell,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let app = App::parse();

    let result = match app.command {
        Command::Issue(cmd) => cmd.run().await,
        Command::Pr(cmd) => cmd.run().await,
        Command::Repo(cmd) => cmd.run().await,
        Command::Label(cmd) => cmd.run().await,
        Command::Milestone(cmd) => cmd.run().await,
        Command::Release(cmd) => cmd.run().await,
        Command::Project(cmd) => cmd.run().await,
        Command::Run(cmd) => cmd.run().await,
        Command::Search(cmd) => cmd.run().await,
        Command::Secret(cmd) => cmd.run().await,
        Command::Variable(cmd) => cmd.run().await,
        Command::Notification(cmd) => cmd.run().await,
        Command::Org(cmd) => cmd.run().await,
        Command::Auth(cmd) => cmd.run().await,
        Command::Config(cmd) => cmd.run().await,
        Command::Browse(cmd) => cmd.run().await,
        Command::Api(cmd) => cmd.run().await,
        Command::Completion(args) => {
            clap_complete::generate(
                args.shell,
                &mut App::command(),
                "gt",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    };

    if let Err(ref e) = result {
        let msg = e.to_string();
        if msg.starts_with("HTTP 401") {
            eprintln!("hint: try `gt auth login`");
        } else if msg.starts_with("HTTP 403") {
            eprintln!("hint: you don't have permission for this operation");
        }
    }

    result
}
