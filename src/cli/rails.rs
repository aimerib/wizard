use super::docker::{
    get_container_id, init_docker,
    utils::{project_hash, remove_container, run_container_command, ExecConfig},
};
use bollard::models::HostConfig;
use bollard::{
    container::{Config, CreateContainerOptions},
    models::Mount,
};
use bollard::{image::CreateImageOptions, models::MountTypeEnum};
use clap::{ArgEnum, Subcommand};
use crossterm::style::Stylize;
use futures_util::TryStreamExt;
use std::{env, error::Error, fs::File, io::Write};

#[derive(Subcommand, Debug)]
#[clap(about = "Execute a rails command in the main project container")]
pub(crate) enum Rails {
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

pub(crate) async fn rails_cmd(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let docker = init_docker();

    let path = env::current_dir()?;
    let project_hash = project_hash(path.to_str().unwrap());
    let project_name = path.file_name().unwrap().to_str().unwrap().to_owned();
    let container_name = format!("{}-{}-{}", project_name, project_name, project_hash);
    let user = format!("{}-user", project_name);

    let id = get_container_id(&docker, &container_name).await?;
    let mut cmd = vec!["rails".to_string()];
    cmd.append(&mut args.clone());
    let cmd = cmd.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
    let run_command = ExecConfig {
        user: Some(&user),
        command_args: &cmd,
        attach_stdin: Some(true),
        ..Default::default()
    };
    run_container_command(&docker, &id, run_command).await?;
    Ok(())
}

pub(crate) async fn rails_new(
    name: String,
    api: bool,
    database: Option<Database>,
) -> Result<(), Box<dyn Error>> {
    // let user = whoami::username();
    let user = format!("{}-user", &name);
    let path = env::current_dir()?;
    let api = if api { "--api" } else { "" };
    let db = if database.is_some() {
        format!(
            "--database={:?}",
            database
                .clone()
                .unwrap()
                .to_possible_value()
                .unwrap()
                .get_name()
        )
        .replace("\"", "")
    } else {
        "--skip-active-record".to_owned()
    };
    let install_sqlite3 = match database {
        Some(Database::Sqlite3) => "RUN apt-get update && apt-get install -y sqlite3".to_owned(),
        _ => "".to_owned(),
    };
    let docker = init_docker();
    const IMAGE: &str = "ruby:3.1";
    let docker_file: (&str, String) = (
        "Dockerfile",
        format!(
            r#"FROM ruby:3.1

WORKDIR /app
RUN groupadd -r {0} -g 1000
RUN useradd -u 1000 -g {0} {0} -m -d /home/{0}
{1}
USER {0}
RUN gem install bundler

COPY Gemfile* /app/

RUN bundle install

COPY . /app
CMD ["tail", "-f", "/dev/null"]"#,
            &user, install_sqlite3
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
          - '3000:3000'
      env_file:
          - .env
      command: bash -c "rm -f tmp/pids/server.pid && bundle exec rails s -p 3000 -b '0.0.0.0'""#,
        &name
    );
    let docker_compose_file = if database.is_some() {
        match database.unwrap() {
            Database::Postgresql => format!(
                r#"{}
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
                docker_compose_file
            ),
            Database::Mysql => format!(
                r#"{}
  db:
      image: mysql:latest
      command:
          - --default-authentication-plugin=mysql_native_password
      environment:
          - MYSQL_ROOT_PASSWORD=root
          - MYSQL_DATABASE=app_development
      ports:
          - "3306:3306"
      volumes:
          - mysql:/var/lib/mysql
volumes:
  mysql:"#,
                docker_compose_file
            ),
            Database::Sqlite3 => docker_compose_file,
        }
    } else {
        docker_compose_file
    };
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
        mounts: Some(vec![Mount {
            target: Some("/tmp".to_string()),
            typ: Some(MountTypeEnum::TMPFS),
            read_only: Some(false),
            ..Default::default()
        }]),
        ..Default::default()
    };

    let ruby_config = Config {
        image: Some(IMAGE),
        tty: Some(true),
        working_dir: Some(&work_dir),
        host_config: Some(host_config),
        ..Default::default()
    };
    let id = docker
        .create_container::<&str, &str>(
            Some(CreateContainerOptions {
                name: "rails-create-container",
            }),
            ruby_config,
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

    let install_rails = ExecConfig {
        user: Some(&user),
        env: Some(vec!["GEM_PATH=/tmp/gem", "GEM_SPEC_CACHE=/tmp/gem/cache"]),
        command_args: &[
            "gem",
            "install",
            "rails",
            "--no-document",
            "--no-user-install",
        ],
        ..Default::default()
    };

    let create_app = ExecConfig {
        user: Some(&user),
        work_dir: Some(&work_dir),
        command_args: &["rails", "new", &name, api, &db],
        attach_stdin: Some(true),
        // we need to set this to make sure the gem cache is created
        // inside the container only in order to not polute the host.
        // since this container is removed after the command is executed,
        // these changes are not persisted.
        env: Some(vec![
            "GEM_PATH=/tmp/gem",
            "GEM_SPEC_CACHE=/tmp/gem/cache",
            "HOME=/tmp",
        ]),
    };
    let project_folder = format!("{}/{}", &work_dir, &name);
    let add_pry = ExecConfig {
        user: Some(&user),
        work_dir: Some(&project_folder),
        command_args: &["bundle", "add", "pry-rails", "--group=development"],
        attach_stdin: Some(true),
        // we need to set this to make sure the gem cache is created
        // inside the container only in order to not polute the host.
        // since this container is removed after the command is executed,
        // these changes are not persisted.
        env: Some(vec![
            "GEM_PATH=/tmp/gem",
            "GEM_SPEC_CACHE=/tmp/gem/cache",
            "HOME=/tmp",
        ]),
    };

    for execution in [
        create_group,
        create_user,
        install_rails,
        create_app,
        add_pry,
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
