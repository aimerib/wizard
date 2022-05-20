use self::{
    docker::{
        compose::{compose_build, compose_down, compose_restart, compose_up, enter_shell},
        docker_status,
    },
    phoenix::{phoenix_cmd, phoenix_new, Phoenix},
    rails::{rails_cmd, Rails},
};
use clap::{ArgEnum, Parser, Subcommand};
use clap_complete::{
    generate,
    shells::{Bash, PowerShell, Zsh},
};
use std::io;
pub(crate) mod docker;
mod greeting;
mod phoenix;
mod rails;
mod traits;

#[derive(Parser, Debug)]
pub(crate) struct Wizard {
    #[clap(long)]
    /// Docker status for the current project
    status: bool,
    #[clap(long, arg_enum)]
    generate_completion: Option<Shells>,
    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(ArgEnum, Debug, Clone)]
enum Shells {
    Bash,
    Zsh,
    Powershell,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[clap(subcommand)]
    Rails(Rails),
    #[clap(subcommand)]
    Phoenix(Phoenix),
    #[clap(flatten)]
    DockerCompose(DockerCompose),
    #[clap(flatten)]
    New(WizardNew),
}
#[derive(Subcommand, Debug)]

enum WizardNew {
    /// Generate a new dockerized app
    New {
        #[clap(subcommand)]
        kind: AppKind,
    },
}
#[derive(Subcommand, Debug, Clone)]
enum AppKind {
    Rails {
        #[clap(help = "The name of the new rails app")]
        name: String,
        #[clap(long, help = "Generate an api only app and skip view generation")]
        api: bool,
        #[clap(arg_enum, long, short, help = "Which database to use")]
        database: Option<rails::Database>,
    },
    Phoenix {
        #[clap(help = "The name of the new phoenix app")]
        name: String,
        // #[clap(long, help = "Generate an api only app and skip view generation")]
        // api: bool,
        // #[clap(arg_enum, long, short, help = "Which database to use")]
        // database: Option<rails::Database>,
    },
    Rust,
}
#[derive(Subcommand, Debug)]
enum DockerCompose {
    /// Start the docker compose project
    Start {
        #[clap(long, short)]
        /// start the project in detached mode
        detached: bool,
    },
    /// Stop the docker compose project
    Stop,
    /// Restart the docker compose project
    /// This will stop and start the project
    Restart,
    /// Build the images in the docker compose project
    Build {
        #[clap(short, long)]
        /// force the build even if the images are up to date
        force: bool,
    },
    /// Run a shell inside a container. If no container
    /// is specified, the container for the first service
    /// listed in the docker-compose file will be used
    Shell {
        #[clap(short, long)]
        /// if provided, the shell will be run in the specified container
        container_name: Option<String>,
        #[clap(short, long)]
        /// if provided, the shell will be run as the specified user
        user: Option<String>,
    },
}
#[tokio::main]
pub(crate) async fn cli_client(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let wizard = Wizard::parse_from(&args);
    if wizard.status {
        docker_status().await?;
    };

    if wizard.generate_completion.is_some() {
        match wizard.generate_completion.unwrap() {
            Shells::Bash => generate(
                Bash,
                &mut <Wizard as clap::CommandFactory>::command(),
                "wizard".to_string(),
                &mut io::stdout(),
            ),
            Shells::Zsh => generate(
                Zsh,
                &mut <Wizard as clap::CommandFactory>::command(),
                "wizard".to_string(),
                &mut io::stdout(),
            ),
            Shells::Powershell => generate(
                PowerShell,
                &mut <Wizard as clap::CommandFactory>::command(),
                "wizard".to_string(),
                &mut io::stdout(),
            ),
        }
        generate(
            Bash,
            &mut <Wizard as clap::CommandFactory>::command(),
            "wizard".to_string(),
            &mut io::stdout(),
        );
    }
    if let Some(command) = wizard.command {
        match command {
            Command::Rails(rails) => match rails {
                Rails::Command(rails) => rails_cmd(rails).await?,
            },
            Command::DockerCompose(dc_opts) => match dc_opts {
                DockerCompose::Start { detached } => {
                    compose_up(detached).await?;
                }
                DockerCompose::Stop => {
                    compose_down().await?;
                }
                DockerCompose::Restart => compose_restart().await,
                DockerCompose::Build { force } => compose_build(force).await?,
                DockerCompose::Shell {
                    container_name,
                    user,
                } => enter_shell(container_name, user).await?,
            },
            Command::New(app_kind) => match app_kind {
                WizardNew::New { kind } => match kind {
                    AppKind::Rails {
                        name,
                        api,
                        database,
                    } => rails::rails_new(name, api, database).await?,
                    AppKind::Phoenix { name } => phoenix_new(name).await?,
                    _ => println!("Not implemented"),
                },
            },
            Command::Phoenix(phoenix) => match phoenix {
                Phoenix::Command(rails) => phoenix_cmd(rails).await?,
            },
        }
    }
    Ok(())
}
