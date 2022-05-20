use bollard::{
    container::{
        AttachContainerResults, Config, CreateContainerOptions, ListContainersOptions,
        NetworkingConfig,
    },
    image::{BuildImageOptions, ListImagesOptions, RemoveImageOptions},
    models::{ContainerState, EndpointSettings, HostConfig, Mount, MountTypeEnum, PortBinding},
    network::{CreateNetworkOptions, ListNetworksOptions},
};
use crossterm::style::Stylize;
use docker_compose_types::{
    Command, Compose, ComposeFile, ComposeVolumes, Environment, Services, TopLevelVolumes, Volumes,
};
use futures_util::{StreamExt, TryStreamExt};
use indicatif::ProgressBar;
// use owo_colors::OwoColorize;
use std::{collections::HashMap, env, error::Error, fs, io::Write};
use tokio_stream::StreamMap;

use super::{
    get_container_id, get_containers, init_docker,
    utils::{project_hash, run_container_command, ExecConfig},
};
use crate::cli::{docker::utils::remove_container, traits::IntoArgs};

pub async fn compose_up(detached: bool) -> Result<(), Box<dyn Error>> {
    let mut started_containers = vec![];
    let path = env::current_dir()?;
    let project_hash = project_hash(path.to_str().unwrap());
    let project_name = path.file_name().unwrap().to_str().unwrap().to_owned();
    let docker = init_docker();
    let Compose {
        services, volumes, ..
    } = parse_docker_compose_file()?;
    let mut dc_volumes = vec![];
    if let Some(TopLevelVolumes::CV(ComposeVolumes(volumes_maps))) = volumes {
        for (volume_name, _volume_options) in volumes_maps {
            dc_volumes.push(volume_name);
        }
    };

    let services = services.expect("Docker-compose file must have services");
    let Services(services_map) = services;
    let existing_networks = docker
        .list_networks(None::<ListNetworksOptions<String>>)
        .await?
        .iter()
        .map(|n| (n.name.clone().unwrap(), n.id.clone().unwrap()))
        .collect::<HashMap<String, String>>();
    let network_name = format!("{}_default", project_name);
    let network_id = match existing_networks.get(network_name.as_str()) {
        Some(id) => id.clone(),
        None => docker
            .create_network(CreateNetworkOptions {
                name: network_name.clone(),
                driver: "bridge".to_string(),
                ..Default::default()
            })
            .await?
            .id
            .unwrap(),
    };

    for (service_name, service_config) in services_map {
        let mut user = Some(format!("{}-user", &project_name));

        let service_config = service_config.expect("Service must have config");

        let container_name = format!("{}-{}-{}", project_name, &service_name, project_hash);

        if container_exists(&container_name).await? {
            if container_running(&container_name).await? {
                continue;
            }
            let pb =
                ProgressBar::new_spinner().with_message(format!("Starting {}", &container_name));
            pb.enable_steady_tick(100);
            let network_name = format!("{}_default", project_name);

            if !docker
                .list_networks(None::<ListNetworksOptions<String>>)
                .await?
                .iter()
                .map(|n| n.name.clone().unwrap())
                .any(|x| x == network_name)
            {
                docker
                    .create_network(CreateNetworkOptions {
                        name: network_name,
                        check_duplicate: true,
                        ..Default::default()
                    })
                    .await?;
            }
            docker
                .start_container::<String>(&container_name, None)
                .await?;
            pb.finish_with_message(format!(
                "{} {} [{}]",
                "✔".green(),
                &container_name,
                "started".green()
            ));
            continue;
        } else {
            let mut endpoints_config = HashMap::new();
            endpoints_config.insert(
                network_name.clone(),
                EndpointSettings {
                    network_id: Some(network_id.clone()),
                    aliases: Some(vec![
                        format!("{}-{}", project_name, service_name),
                        service_name.to_string(),
                    ]),
                    ..Default::default()
                },
            );
            let env_file = match service_config.env_file {
                Some(docker_compose_types::EnvFile::List(env_files_list)) => Some(env_files_list),
                Some(docker_compose_types::EnvFile::Simple(env_file)) => Some(vec![env_file]),
                None => None,
            };
            let networking_config = NetworkingConfig { endpoints_config };
            let host_config = HostConfig {
                mounts: extract_volumes(service_config.volumes, dc_volumes.clone()),
                port_bindings: extract_ports(service_config.ports.clone()),
                network_mode: Some(network_name.clone()),
                ..Default::default()
            };
            let image_name = if service_config.image.is_some() {
                user = None;
                service_config.image.clone().unwrap()
            // Otherwise create an image from dockerfile located in the project root
            } else {
                build_image_from_docker_file(service_name.clone()).await?;
                service_name.clone()
            };
            let cmd = match &service_config.command {
                Some(Command::Simple(cmd)) => cmd.try_into_args().ok(),
                None => None,
                _ => panic!("Unsupported command"),
            };
            let mut exposed_ports = HashMap::new();

            if let Some(dc_ports) = service_config.ports.clone() {
                for ports in dc_ports {
                    let empty = HashMap::new();

                    let port = format!("{}/tcp", ports.split(':').next().unwrap());
                    exposed_ports.insert(port, empty.clone());
                }
            };
            let container_config = Config {
                user,
                image: Some(image_name),
                host_config: Some(host_config),
                exposed_ports: Some(exposed_ports),
                cmd,
                networking_config: Some(networking_config),
                env: extract_env((service_config.environment, env_file)),
                ..Default::default()
            };
            let container_name = format!("{}-{}-{}", project_name, service_name, project_hash);
            let container_id = docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: &container_name,
                    }),
                    container_config,
                )
                .await?
                .id;
            let pb =
                ProgressBar::new_spinner().with_message(format!("Starting {}", &container_name));
            pb.enable_steady_tick(100);
            let container_result = docker.start_container::<String>(&container_id, None).await;
            if container_result.is_ok() {
                started_containers.push(container_name.clone());
                pb.finish_with_message(format!(
                    "{} {} [{}]",
                    "✔".green(),
                    &container_name,
                    "started".green()
                ));
            } else {
                pb.abandon_with_message(format!(
                    "{} {} [{}]",
                    "✘".red(),
                    &container_name,
                    "failed".red()
                ));
                println!("{}", container_result.err().unwrap().to_string().red());
            }
        }
    }
    if detached {
        return Ok(());
    } else {
        if started_containers.is_empty() {
            println!("[{}]::Status - Something went wrong", "Wizard".red());
            std::process::exit(1);
        }
        let options = Some(bollard::container::AttachContainerOptions::<String> {
            stdout: Some(true),
            stderr: Some(true),
            stream: Some(true),
            logs: Some(true),
            detach_keys: Some("ctrl-c".to_string()),
            ..Default::default()
        });
        let mut map = StreamMap::new();
        for container in &started_containers {
            let container_name = container.clone();
            let AttachContainerResults { output, .. } = docker
                .attach_container(&container_name, options.clone())
                .await?;
            map.insert(container_name, output);
        }
        let decorator_length = if started_containers.len() > 1 {
            started_containers
                .iter()
                .cloned()
                .map(|x| x.len())
                .max()
                .unwrap()
        } else {
            started_containers[0].len()
        };
        loop {
            tokio::select! {
                Some((ctnr, msg)) = map.next() => {
                    let msg_bytes = msg?.into_bytes();
                    let msg_string = String::from_utf8_lossy(&msg_bytes);
                    let messages = msg_string.split('\n').map(|string|{
                        let screen_width = crossterm::terminal::size().unwrap().0 as usize;

                        if (string.len() + decorator_length) > screen_width {
                            let test = string.split_at(screen_width - decorator_length - 6_usize);
                            vec![test.0, test.1]
                        } else {
                            let test = string.split_at(string.len());
                            vec![test.0]
                        }
                    }).flatten().collect::<Vec<&str>>();
                    for message in messages {
                        if !message.is_empty() {
                            let container = format!("{:<width$}", ctnr.clone(), width = decorator_length);

                            let container = if ctnr.len() < decorator_length {
                                container.yellow()
                            } else {
                                container.cyan()
                            };
                            println!("{} | {}", container, message);
                        }
                    }
                }
                else => break,
            }
        }
    }
    Ok(())
}

async fn container_exists(container_name: &str) -> Result<bool, Box<dyn Error>> {
    let docker = init_docker();
    let options = Some(ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    });
    let existing_containers = docker.list_containers(options).await?;
    for container in existing_containers {
        if container
            .names
            .clone()
            .unwrap()
            .contains(&format!("/{}", container_name))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn container_running(container_name: &str) -> Result<bool, Box<dyn Error>> {
    let docker = init_docker();
    let options = Some(ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    });
    let existing_containers = docker.list_containers(options).await?;
    for container in existing_containers {
        if container
            .names
            .clone()
            .unwrap()
            .contains(&format!("/{}", container_name))
        {
            if let Some(status) = &container.state {
                if status == "running" {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub(crate) async fn compose_restart() {
    let docker = init_docker();
    let containers = get_containers().await.unwrap();

    for container in containers {
        let pb = ProgressBar::new_spinner().with_message(format!("Restarting {}", &container));
        pb.enable_steady_tick(100);
        if docker.restart_container(&container, None).await.is_ok() {
            pb.finish_with_message(format!(
                "{} {} [{}]",
                "✔".green(),
                &container,
                "restarted".green()
            ));
        } else {
            pb.abandon_with_message(format!("{} {} [{}]", "✘".red(), &container, "failed".red()));
        }
    }
}

pub(crate) async fn compose_build(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    compose_down().await?;
    let Compose { services, .. } = parse_docker_compose_file()?;
    let services = services.expect("Docker-compose file must have services");
    let Services(services_map) = services;
    for (service_name, service_config) in services_map {
        let service_config = service_config.expect("Service must have config");
        if service_config.image.is_none() {
            let pb = ProgressBar::new_spinner().with_message("Building containers");
            pb.enable_steady_tick(100);
            if force {
                remove_image(&service_name).await?;
            }
            if build_image_from_docker_file(service_name.to_string())
                .await
                .is_ok()
            {
                pb.finish_with_message(format!("{} image [{}]", "✔".green(), "built".green()));
            } else {
                pb.abandon_with_message(format!("{} image [{}]", "✘".red(), "failed".red()));
            }
        }
    }
    Ok(())
}

async fn remove_image(image_name: &str) -> Result<(), Box<dyn Error>> {
    let docker = init_docker();
    let list_options = Some(ListImagesOptions::<String> {
        all: true,
        ..Default::default()
    });
    let remove_options = Some(RemoveImageOptions {
        force: true,
        ..Default::default()
    });
    let existing_images = docker.list_images(list_options).await?;
    for image in existing_images {
        if image
            .repo_tags
            .clone()
            .contains(&format!("{}:latest", &image_name))
        {
            let pb = ProgressBar::new_spinner().with_message(format!("Removing {}", &image_name));
            pb.enable_steady_tick(100);
            if docker
                .remove_image(image_name, remove_options, None)
                .await
                .is_ok()
            {
                pb.finish_with_message(format!("{} image [{}]", "✔".green(), "removed".green()));
            } else {
                pb.abandon_with_message(format!("{} image [{}]", "✘".red(), "failed".red()));
            }
        }
    }
    Ok(())
}

pub async fn compose_down() -> Result<(), Box<dyn Error>> {
    let path = env::current_dir()?;
    let project_name = path.file_name().unwrap().to_str().unwrap().to_owned();
    let network_name = format!("{}_default", project_name);
    let docker = init_docker();
    let containers = get_containers().await?;
    for container in containers.iter() {
        let pb = ProgressBar::new_spinner().with_message(format!("Stopping {}", &container));
        pb.enable_steady_tick(100);
        let inspect_container = docker.inspect_container(container, None).await?;
        if let Some(ContainerState {
            running: Some(true),
            ..
        }) = inspect_container.state
        {
            docker.stop_container(container, None).await?;
        }
        docker.remove_container(container, None).await?;
        pb.finish_with_message(format!(
            "{} {} [{}]",
            "✔".green(),
            &container,
            "stopped".green()
        ));
    }

    if docker
        .list_networks(None::<ListNetworksOptions<String>>)
        .await?
        .iter()
        .map(|n| n.name.clone().unwrap())
        .any(|x| x == network_name)
    {
        docker.remove_network(&network_name).await?;
    }
    Ok(())
}

pub(crate) fn get_service_names_from_compose_file() -> Result<Vec<String>, Box<dyn Error>> {
    let Compose { services, .. } = parse_docker_compose_file()?;
    let services = services.expect("Docker-compose file must have services");
    let Services(services_map) = services;
    let path = env::current_dir()?;
    let project_hash = project_hash(path.to_str().unwrap());
    let project_name = path.file_name().unwrap().to_str().unwrap().to_owned();
    let mut service_names = vec![];
    for (service_name, _) in services_map {
        service_names.push(service_name);
    }
    let service_names = service_names
        .into_iter()
        .map(|s| format!("{}-{}-{}", project_name, s, project_hash))
        .collect();
    Ok(service_names)
}

fn extract_ports(
    dc_ports: Option<Vec<String>>,
) -> Option<HashMap<String, Option<Vec<PortBinding>>>> {
    if let Some(ports_vec) = dc_ports {
        // ports are in an array of str (e.g. ["8080:8080"]) where [host:container]
        // coming from docker-compose we need to create an Option<HashMap<T, HashMap<(), ()>>>
        let mut port_bindings = HashMap::new();
        for ports in ports_vec {
            let ports = ports.split(':').collect::<Vec<_>>();
            let host_port = ports[0];
            let container_port = format!("{}/tcp", ports[1]);
            port_bindings.insert(
                container_port,
                Some(vec![PortBinding {
                    host_ip: Some(String::from("0.0.0.0")),
                    host_port: Some(String::from(host_port)),
                }]),
            );
        }
        Some(port_bindings)
    } else {
        None
    }
}

fn extract_volumes(
    dc_service_volumes: Option<Volumes>,
    dc_volumes: Vec<String>,
) -> Option<Vec<Mount>> {
    let path = env::current_dir().unwrap().to_str().unwrap().to_string();
    if let Some(Volumes::Simple(volumes_vec)) = dc_service_volumes {
        let mut mounts = vec![];
        for volumes in volumes_vec {
            let volumes = volumes.split(':').collect::<Vec<_>>();
            let host_path = if volumes[0] == "." {
                path.as_str()
            } else {
                volumes[0]
            };
            let container_path = volumes[1];
            let typ = if dc_volumes.contains(&host_path.to_string()) {
                MountTypeEnum::VOLUME
            } else {
                MountTypeEnum::BIND
            };
            mounts.push(Mount {
                target: Some(container_path.to_string()),
                source: Some(host_path.to_string()),
                typ: Some(typ),
                consistency: Some(String::from("default")),
                ..Default::default()
            });
        }
        Some(mounts)
    } else {
        None
    }
}

fn extract_env(dc_env: (Option<Environment>, Option<Vec<String>>)) -> Option<Vec<String>> {
    let mut env = vec![];
    if let Some(Environment::KvPair(map)) = dc_env.0 {
        for (key, value) in map {
            let value = match value {
                Some(value) => value,
                None => "".to_string(),
            };
            env.push(format!("{}={}", key, value));
        }
    }

    if let Some(env_vec) = dc_env.1 {
        for env_var in env_vec {
            let env_file_string = std::fs::read_to_string(&env_var)
                .unwrap_or_else(|_| panic!("Could not read env file {}", &env_var));
            for line in env_file_string.lines() {
                let parts = line.split_once("=");
                if let Some((key, value)) = parts {
                    env.push(format!("{}={}", key, value));
                }
            }
        }
    }

    if !env.is_empty() {
        Some(env)
    } else {
        None
    }
}

async fn build_image_from_docker_file(service_name: String) -> Result<(), Box<dyn Error>> {
    let docker = init_docker();
    let curr_dir = env::current_dir()?;
    let dockerfile_path = curr_dir.join("Dockerfile");
    let dockerfile = std::fs::read_to_string(&dockerfile_path).expect(
        "Could not read Dockerfile. Make sure to run this command from the root of the project.",
    );
    let mut header = tar::Header::new_gnu();
    header.set_path("Dockerfile").unwrap();
    header.set_size(dockerfile.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    let current_dir = env::current_dir()?;
    let context = fs::read_dir(current_dir)?;
    let mut tar = tar::Builder::new(Vec::new());
    tar.append(&header, dockerfile.as_bytes()).unwrap();
    for entry in context {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().unwrap();
        if file_name.to_str().unwrap() == "Dockerfile" {
            continue;
        }
        tar.append_path(&file_name).unwrap();
    }

    let uncompressed = tar.into_inner().unwrap();
    let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    c.write_all(&uncompressed).unwrap();
    let compressed = c.finish().unwrap();

    let options = BuildImageOptions {
        dockerfile: "Dockerfile",
        t: &service_name,
        rm: true,
        ..Default::default()
    };
    let mut building = docker.build_image(options, None, Some(compressed.into()));
    let pb = ProgressBar::new_spinner().with_message(format!("building {}", &service_name));
    pb.enable_steady_tick(100);
    while let Some(build_info) = building.try_next().await? {
        if let Some(stream) = build_info.stream.clone() {
            if stream != "\n" {
                pb.println(format!(
                    "[{}]::Status | {}",
                    "Wizard".cyan(),
                    stream.trim_start_matches('\n')
                ));
            }
        }
        if let Some(err) = build_info.error {
            pb.abandon_with_message(format!("{} {} [{}]", "✘".red(), &err, "failed".red()));
            std::process::exit(1);
        }
    }
    pb.finish_with_message(format!(
        "{} {} [{}]",
        "✔".green(),
        &service_name,
        "built".green()
    ));
    Ok(())
}

pub(crate) fn parse_docker_compose_file() -> Result<Compose, Box<dyn Error>> {
    let docker_file = std::fs::read_to_string("docker-compose.yaml");
    let docker_file = if docker_file.is_ok() {
        docker_file
    } else {
        std::fs::read_to_string("docker-compose.yml")
    };
    let docker_file = docker_file?;
    let dc: ComposeFile = serde_yaml::from_str(docker_file.as_str()).unwrap();
    let docker_compose = match dc {
        ComposeFile::V2Plus(dc) => dc,
        _ => panic!("Unsupported docker-compose version. Please use v3 or higher"),
    };
    Ok(docker_compose)
}

pub(crate) async fn enter_shell(
    container_name: Option<String>,
    user: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let docker = init_docker();
    let path = env::current_dir()?;
    let project_hash = project_hash(path.to_str().unwrap());
    let project_name = path.file_name().unwrap().to_str().unwrap().to_owned();
    let user = if user.is_some() {
        user.unwrap()
    } else {
        format!("{}-user", project_name)
    };

    let id = match container_name {
        Some(container_name) => get_container_id(&docker, &container_name).await?,
        None => format!("{}-{}-{}", project_name, project_name, project_hash),
    };
    let docker = init_docker();
    let enter_shell = ExecConfig {
        attach_stdin: Some(true),
        user: Some(&user),
        command_args: &["bash"],
        ..Default::default()
    };

    let run_result = run_container_command(&docker, &id, enter_shell.clone()).await;
    if run_result.is_err() {
        let err_message = run_result.unwrap_err().to_string();
        remove_container(&id).await;
        println!(
            "\n[{}] - failed to execute command: {}",
            "error".dark_red(),
            enter_shell.command_args.join(" ").cyan(),
        );
        println!(
            "[{}] - Error from command: {}",
            "error".dark_red(),
            err_message.clone().red(),
        );
        std::process::exit(1);
    }
    Ok(())
}
