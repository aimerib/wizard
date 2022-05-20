// use bollard::Docker;
// use clap::StructOpt;
// use cli::docker::compose::parse_docker_compose_file;
// use cli::docker::init_docker;
// use cli::docker::utils::project_hash;
use cli::cli_client;
// use crossterm::style::Stylize;
// use docker_compose_types::Compose;
// use crossterm::{
//     event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
//     execute,
//     terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
// };
use owo_colors::OwoColorize;
use std::env;

use std::error::Error;
// use std::path::PathBuf;
// use tui::{
//     backend::{Backend, CrosstermBackend},
//     layout::{Constraint, Direction, Layout},
//     style::{Color, Style},
//     widgets::{Block, Borders, Paragraph},
//     Frame, Terminal,
// };

mod cli;

// #[tokio::main]
fn main() -> Result<(), Box<dyn Error>> {
    let args_vec = env::args().collect::<Vec<_>>();
    if args_vec.len() > 1 {
        cli_client(args_vec)
    } else {
        // let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel::<AppEvent>();
        // let app = Arc::new(Mutex::new(App::new("Wizard".to_string())));
        // let cloned_app = Arc::clone(&app);
        // std::thread::spawn(move || {
        //     ui_tokio_runner(&app, sync_io_rx);
        // });
        // let app = CliApp::new().build(args_vec);
        println!(
            "Wizard::{} - {}",
            "Warning".yellow(),
            "TUI not yet implemented".red()
        );
        println!(
            "Wizard::{} - Please use the CLI. See `wizard --help`",
            "Warning".yellow()
        );
        // TODO: Implement TUI
        // tui_client(&cloned_app)
        Ok(())
    }
}
// TODO: Implement TUI
// fn tui_client(app: &Arc<Mutex<App>>) -> Result<(), Box<dyn Error>> {
//     // setup terminal
//     enable_raw_mode()?;
//     let mut stdout = io::stdout();
//     execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
//     let backend = CrosstermBackend::new(stdout);
//     let mut terminal = Terminal::new(backend)?;

//     // create app and run it
//     // let res = run_app(&mut terminal);

//     loop {
//         terminal.draw(|f| {
//             let chunks = Layout::default()
//                 .direction(Direction::Vertical)
//                 .constraints(
//                     [
//                         Constraint::Percentage(10),
//                         Constraint::Percentage(80),
//                         Constraint::Percentage(10),
//                     ]
//                     .as_ref(),
//                 )
//                 .split(f.size());

//             let block = Block::default().title("Block").borders(Borders::ALL);
//             f.render_widget(block, chunks[0]);
//             let text = Paragraph::new(app.lock().unwrap().text.clone())
//                 .block(Block::default().title("Paragraph").borders(Borders::ALL))
//                 .style(Style::default().fg(Color::White).bg(Color::Black));
//             f.render_widget(text, chunks[1]);
//             let block = Block::default().title("Block 2").borders(Borders::ALL);
//             f.render_widget(block, chunks[2]);
//         })?;

//         if let Event::Key(key) = event::read()? {
//             if let KeyCode::Char('q') = key.code {
//                 break;
//             }
//         }
//     }

//     // restore terminal
//     disable_raw_mode()?;
//     execute!(
//         terminal.backend_mut(),
//         LeaveAlternateScreen,
//         DisableMouseCapture
//     )?;
//     terminal.show_cursor()?;
//     Ok(())
// }

// fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
//     loop {
//         terminal.draw(|f| ui(f))?;

//         if let Event::Key(key) = event::read()? {
//             if let KeyCode::Char('q') = key.code {
//                 return Ok(());
//             }
//         }
//     }
// }
// #[tokio::main]
// async fn ui_tokio_runner(app: &Arc<Mutex<App>>, rx: std::sync::mpsc::Receiver<AppEvent>) {
//     while let Ok(io_event) = rx.recv() {
//         match io_event {
//             AppEvent::Quit => {
//                 break;
//             }
//             AppEvent::ChangeText(text) => {
//                 app.lock().unwrap().set_text(text);
//             }
//         };
//     }
// }

// fn ui<B: Backend>(f: &mut Frame<B>) {
//     let chunks = Layout::default()
//         .direction(Direction::Vertical)
//         .constraints(
//             [
//                 Constraint::Percentage(10),
//                 Constraint::Percentage(80),
//                 Constraint::Percentage(10),
//             ]
//             .as_ref(),
//         )
//         .split(f.size());

//     let block = Block::default().title("Block").borders(Borders::ALL);
//     f.render_widget(block, chunks[0]);
//     let block = Block::default().title("Block 2").borders(Borders::ALL);
//     f.render_widget(block, chunks[2]);
// }

// // control state from here. have docker, docker-compose, path, name, etc all here and initialize with ::new()
// // maybe use builder patter to allow for configuration, and on build I can check if config is set
// // and if not I can use default values

// #[derive(Debug, Default)]
// struct CliApp {
//     command: Option<Wizard>,
//     docker: Option<Docker>,
//     docker_compose: Option<Compose>,
//     path: PathBuf,
//     project_name: Option<String>,
//     user: Option<String>,
//     project_hash: String,
// }
// impl CliApp {
//     fn new() -> Self {
//         CliApp::default()
//     }
//     fn build(self, args: Vec<String>) -> Self {
//         let docker_compose = if self.docker_compose.is_some() {
//             self.docker_compose
//         } else {
//             let parse_result = parse_docker_compose_file();
//             if parse_result.is_err() {
//                 println!("here");
//                 None
//             } else {
//                 Some(parse_result.unwrap())
//             }
//         };
//         let path = env::current_dir();
//         let path = if path.is_err() {
//             println!("[{}] - Error reading current directory", "error".red());
//             std::process::exit(1);
//         } else {
//             path.unwrap()
//         };
//         let docker = init_docker();

//         let project_hash = project_hash(path.to_str().unwrap());
//         let project_name = if docker_compose.is_some() {
//             Some(path.file_name().unwrap().to_str().unwrap().to_owned())
//         } else {
//             None
//         };
//         let user = if docker_compose.is_some() {
//             Some(format!("{}-user", &project_name.as_ref().unwrap()))
//         } else {
//             None
//         };
//         let command = Some(Wizard::parse_from(args));
//         CliApp {
//             command,
//             docker: Some(docker),
//             docker_compose,
//             path,
//             project_name,
//             user,
//             project_hash,
//         }
//     }
// }
// enum AppEvent {
//     Quit,
//     ChangeText(String),
// }
