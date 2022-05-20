use bollard::models::ContainerSummary;
// use bollard::models::ContainerSummaryInner;
use bollard::{container::ListContainersOptions, Docker};
use crossterm::style::Stylize;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

pub(crate) mod compose;
pub(crate) mod utils;

use self::compose::get_service_names_from_compose_file;

pub(crate) async fn docker_status() -> Result<(), Box<dyn std::error::Error>> {
    let docker = init_docker();
    let project_containers = get_service_names_from_compose_file()?;
    let project_containers: Vec<&str> = project_containers.iter().map(|s| s as &str).collect();
    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("name", project_containers.clone());

    let running_containers = &docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;

    if running_containers.is_empty() {
        for container in project_containers {
            println!(
                "[Wizard]::Status - {} [{}]",
                container.cyan(),
                "stopped".red()
            );
        }
        println!("[Wizard]::Status - {}", "Project not running".red());
    } else {
        let containers_summary_vec = running_containers
            .iter()
            .map(|container| ContainerSummaryInnerWrapper {
                inner: container.clone(),
            })
            .collect::<Vec<ContainerSummaryInnerWrapper>>();
        let stopped_containers: Vec<_> = project_containers
            .iter()
            .filter(|item| {
                !containers_summary_vec
                    .iter()
                    .map(|item| item.name())
                    .any(|x| x == item.to_string())
            })
            .collect();
        for container in containers_summary_vec {
            println!("[Wizard]::Status - {}", container);
        }
        if !stopped_containers.is_empty() {
            for container in stopped_containers {
                println!(
                    "[Wizard]::Status - {} [{}]",
                    container.cyan(),
                    "stopped".red()
                );
            }
            println!(
                "[Wizard]::{} - This project defines services currently not running.",
                "Warning".yellow(),
            );
            println!(
                "[Wizard]::{} - If this is intentional this message can be safely ignored",
                "Warning".yellow(),
            );
        }
    }
    Ok(())
}

pub async fn get_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // get only project containers
    let docker = init_docker();
    let project_containers = get_service_names_from_compose_file()?;
    let project_containers: Vec<&str> = project_containers.iter().map(|s| s as &str).collect();
    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("name", project_containers.clone());
    let containers = &docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;
    let mut container_names = vec![];
    for container in containers {
        container_names.push(container.names.clone().unwrap());
    }
    let container_names = container_names
        .into_iter()
        .flatten()
        .map(|s| s.trim_start_matches('/').to_string())
        .collect();
    Ok(container_names)
}

pub async fn get_container_id(
    docker: &Docker,
    container_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // get only project containers
    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("status", vec!["running"]);
    list_container_filters.insert("name", vec![container_name]);

    let containers = &docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;
    if let Some(container) = containers.first() {
        Ok(container.id.clone().unwrap())
    } else {
        Err("Container not found".into())
    }
}

pub(crate) fn init_docker() -> Docker {
    match Docker::connect_with_socket_defaults() {
        Ok(docker) => docker,
        Err(e) => {
            panic!("{}", e);
        }
    }
}

#[derive(Debug)]
pub(crate) struct ContainerSummaryInnerWrapper {
    pub(crate) inner: ContainerSummary,
}

impl ContainerSummaryInnerWrapper {
    pub(crate) fn name(&self) -> String {
        self.inner
            .names
            .as_ref()
            .unwrap()
            .clone()
            .iter()
            .map(|name| name.strip_prefix('/').unwrap())
            .collect::<Vec<&str>>()
            .join(", ")
    }
}

impl Display for ContainerSummaryInnerWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let name = &self
            .inner
            .names
            .as_ref()
            .unwrap()
            .clone()
            .iter()
            .map(|name| name.strip_prefix('/').unwrap())
            .collect::<Vec<&str>>()
            .join(", ");
        let state = &self.inner.state.as_ref().unwrap();
        let image = &self.inner.image.as_ref().unwrap();

        write!(f, "{}", name.cyan())?;

        match &***state {
            "running" => write!(f, " [{}]", state.green())?,
            "paused" => write!(f, " [{}]", state.yellow())?,
            "exited" => write!(f, " [{}]", state.red())?,
            _ => write!(f, " [{}]", state)?,
        };

        write!(f, " - Image: {}", image)
    }
}
