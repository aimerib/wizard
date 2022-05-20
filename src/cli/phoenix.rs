use super::docker::{
    get_container_id, init_docker,
    utils::{project_hash, remove_container, run_container_command, ExecConfig},
};
use bollard::container::{Config, CreateContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use clap::{ArgEnum, Subcommand};
use crossterm::style::Stylize;
use futures_util::TryStreamExt;
use std::{env, error::Error, fs::File, io::Write};

#[derive(Subcommand, Debug)]
#[clap(about = "Execute a phoenix command in the main project container")]
pub(crate) enum Phoenix {
    #[clap(external_subcommand)]
    Command(Vec<String>),
}
#[derive(Debug, ArgEnum, Clone)]
pub(crate) enum DbArgs {
    Create,
    Drop,
    Migrate,
    Reset,
    Rollback,
    Seed,
    Setup,
    Prepare,
}

#[derive(Debug, ArgEnum, Clone)]
pub(crate) enum Database {
    Postgresql,
    Mysql,
    Sqlite3,
}

pub(crate) async fn phoenix_cmd(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let docker = init_docker();

    let path = env::current_dir()?;
    let project_hash = project_hash(path.to_str().unwrap());
    let project_name = path.file_name().unwrap().to_str().unwrap().to_owned();
    let container_name = format!("{}-{}-{}", project_name, project_name, project_hash);
    let user = format!("{}-user", project_name);
    let id = get_container_id(&docker, &container_name).await?;
    let cmd = args.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
    let run_command = ExecConfig {
        user: Some(&user),
        command_args: &cmd,
        attach_stdin: Some(true),
        ..Default::default()
    };
    run_container_command(&docker, &id, run_command).await?;
    Ok(())
}

pub(crate) async fn phoenix_new(name: String) -> Result<(), Box<dyn Error>> {
    let user = whoami::username();
    let path = env::current_dir()?;
    let docker = init_docker();
    const IMAGE: &str = "elixir:1.13-slim";
    let docker_file: (&str, String) = (
        "Dockerfile",
        format!(
            r#"FROM elixir:1.13

ENV MIX_HOME=/.mix
RUN mkdir /app
RUN apt-get update && apt-get install inotify-tools -y
RUN groupadd -r {0} -g 1000
RUN useradd -u 1000 -g {0} {0} -m -d /home/{0}
WORKDIR /app
COPY . .
RUN mix local.hex --force
RUN mix deps.get
RUN mix local.rebar --force
RUN mix do compile

CMD ["tail", "-f", "/dev/null"]"#,
            format!("{}-user", &name)
        ),
    );
    let docker_compose_file: String = format!(
        r#"version: '3.6'
services:
  {0}:
      build:
          context: .
      volumes:
          - .:/app
      ports:
          - '4000:4000'
      env_file:
          - .env
      command: mix phx.server
  db:
      image: postgres:latest
      volumes:
          - db-data:/var/lib/postgresql/data
      ports:
          - 5432:5432
      environment:
          POSTGRES_USER: postgres
          POSTGRES_PASSWORD: postgres
volumes:
  db-data:"#,
        &name
    );

    let docker_compose_file = ("docker-compose.yml", docker_compose_file);

    docker
        .create_image(
            Some(CreateImageOptions {
                from_image: IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;
    let work_dir = format!("/home/{}", &user);
    let host_config = HostConfig {
        binds: Some(vec![format!(
            "{}:{}:rw",
            &path.to_str().unwrap(),
            &work_dir
        )]),
        ..Default::default()
    };

    let elixir_config = Config {
        image: Some(IMAGE),
        tty: Some(true),
        working_dir: Some(&work_dir),
        host_config: Some(host_config),
        ..Default::default()
    };
    let id = docker
        .create_container::<&str, &str>(
            Some(CreateContainerOptions {
                name: "phoenix-create-container",
            }),
            elixir_config,
        )
        .await?
        .id;
    docker.start_container::<String>(&id, None).await?;

    let create_group = ExecConfig {
        command_args: &["groupadd", "-r", &user, "-g", "1000"],
        ..Default::default()
    };

    let create_user = ExecConfig {
        command_args: &["useradd", "-u", "1000", "-g", &user, &user],
        ..Default::default()
    };

    let install_hex = ExecConfig {
        user: Some(&user),
        command_args: &["mix", "local.hex", "--force"],
        ..Default::default()
    };

    let install_phoenix = ExecConfig {
        user: Some(&user),
        command_args: &["mix", "archive.install", "hex", "phx_new", "--force"],
        ..Default::default()
    };

    let create_app = ExecConfig {
        user: Some(&user),
        work_dir: Some(&work_dir),
        command_args: &["mix", "phx.new", &name, "--install"],
        attach_stdin: Some(true),
        ..Default::default()
    };

    for execution in [
        create_group,
        create_user,
        install_hex,
        install_phoenix,
        create_app,
    ] {
        let run_result = run_container_command(&docker, &id, execution.clone()).await;
        if run_result.is_err() {
            let err_message = run_result.unwrap_err().to_string();
            remove_container(&id).await;
            println!(
                "\n[{}] - failed to execute command: {}",
                "error".dark_red(),
                execution.command_args.join(" ").cyan(),
            );
            println!(
                "[{}] - Error from command: {}",
                "error".dark_red(),
                err_message.clone().red(),
            );
            std::process::exit(1);
        }
    }
    for config_file in [docker_file, docker_compose_file, (".env", "".to_string())] {
        if let Ok(mut file) = File::create(format!("{}/{}", &name, &config_file.0)) {
            let file_result = file.write_all(config_file.1.as_bytes());
            if file_result.is_err() {
                remove_container(&id).await;
                println!(
                    "[{}] - Could not write to file: {} - {}",
                    "error".dark_red(),
                    config_file.0.cyan(),
                    file_result.unwrap_err().to_string().red(),
                );
                std::process::exit(1);
            }
        } else {
            remove_container(&id).await;
            println!(
                "[{}] - Could not create file: {} - {} {} {}",
                "error".dark_red(),
                config_file.0.cyan(),
                "project folder".red(),
                &name.bold().red(),
                "does not exist. Likely due to errors creating project".red()
            );
            std::process::exit(1);
        }
    }

    remove_container(&id).await;
    Ok(())
}
