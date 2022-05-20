use std::{
    error::Error,
    io::{stdout, Write},
    time::Duration,
};

use async_std::{
    io::{stdin, ReadExt},
    prelude::StreamExt,
};
use bollard::{
    container::RemoveContainerOptions,
    exec::{CreateExecOptions, ResizeExecOptions, StartExecResults},
    Docker,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use sha2::{Digest, Sha256};
use tokio::{io::AsyncWriteExt, spawn, time::sleep};

use super::init_docker;

#[derive(Default, Clone)]
pub(crate) struct ExecConfig<'a> {
    pub(crate) user: Option<&'a str>,
    pub(crate) work_dir: Option<&'a str>,
    pub(crate) command_args: &'a [&'a str],
    pub(crate) attach_stdin: Option<bool>,
    pub(crate) env: Option<Vec<&'a str>>,
}

pub(crate) async fn run_container_command(
    docker: &Docker,
    id: &str,
    config: ExecConfig<'_>,
) -> Result<(), Box<dyn Error>> {
    let tty_size = size()?;

    let ExecConfig {
        user,
        work_dir,
        command_args,
        attach_stdin,
        env,
    } = config;
    let cmd: Vec<&str> = command_args.iter().copied().collect();
    let attach_stdin = attach_stdin == Some(true);
    let execution = docker
        .create_exec(
            id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                attach_stdin: Some(attach_stdin),
                tty: Some(true),
                user,
                env,
                working_dir: work_dir,
                cmd: Some(cmd),
                ..Default::default()
            },
        )
        .await?
        .id;

    if let StartExecResults::Attached {
        mut output,
        mut input,
    } = docker.start_exec(&execution, None).await?
    {
        if attach_stdin {
            spawn(async move {
                let mut stdin = stdin().bytes();

                loop {
                    if let Some(Ok(byte)) = stdin.next().await {
                        input.write_all(&[byte]).await.ok();
                    } else {
                        sleep(Duration::from_nanos(10)).await;
                    }
                }
            });
        };
        enable_raw_mode()?;
        let mut stdout = stdout();
        let mut stdout_text = vec![];
        while let Some(Ok(output)) = output.next().await {
            stdout_text = vec![output.clone().into_bytes()];
            stdout.write_all(output.into_bytes().as_ref())?;
            stdout.flush()?;
        }

        let inspect_exec = docker.inspect_exec(&execution).await?;

        if inspect_exec.exit_code.is_none() {
            docker
                .resize_exec(
                    &execution,
                    ResizeExecOptions {
                        height: tty_size.1,
                        width: tty_size.0,
                    },
                )
                .await?;
        }
        disable_raw_mode()?;
        if let Some(code) = inspect_exec.exit_code {
            if code != 0 {
                return Err(String::from_utf8(stdout_text[0].to_vec()).unwrap().into());
            }
        }
    } else {
        unreachable!();
    }
    Ok(())
}

pub(crate) fn project_hash(folder: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(folder);
    let result = hasher.finalize();
    result
        .into_iter()
        .take(6)
        .map(|c| format!("{:x}", c))
        .collect::<Vec<String>>()
        .join("")
}

pub(crate) async fn remove_container(id: &str) {
    let docker = init_docker();
    let remove_result = docker
        .remove_container(
            id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;
    if remove_result.is_err() {
        println!("Error removing {}: {}", id, remove_result.unwrap_err());
        println!(
            "You may need to remove the container manually using \"docker rm {}\"",
            id
        );
    }
}
