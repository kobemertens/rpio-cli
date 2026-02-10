use ansi_term::Style;
use anyhow::Result;
use anyhow::bail;
use chrono::Utc;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use indicatif::ProgressBar;
use indicatif::ProgressFinish;
use indicatif::ProgressStyle;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, Display, EnumIter, EnumString};
use tempfile::NamedTempFile;

#[derive(Parser)]
#[command(name = "rpm")]
#[command(about = "RPM CLI tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Servers {
        // #[command(subcommand)]
        // command: ServersCommand,
    },
    Backups {
        #[command(subcommand)]
        command: BackupsCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum ServersCommand {
    List,
    Refresh,
    Search,
    Tunnel,
}

// An operation on a single application
#[derive(Debug, Clone, Copy, EnumIter, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
enum ApplicationCommand {
    Tunnel,
    RestoreBackup,
    RestoreFiles,
}

#[derive(Subcommand)]
enum BackupsCommand {
    Restore,
}

#[derive(Subcommand)]
enum ConfigCommand {
    Init,
}

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("com", "redpencil", "semanticworks-cli")
        .expect("Could not determine config directory")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub cache_dir: PathBuf,
    pub ignore_hosts: Vec<String>,
}

fn build_fzf_lines(cache: &ServersCache) -> Vec<String> {
    let dim = Style::new().dimmed();

    let mut lines = Vec::new();

    for (host, server) in &cache.servers {
        for folder in &server.data_folders {
            let line = format!("{}:{}", folder.path, dim.paint(host));
            lines.push(line);
        }
    }

    lines
}

fn parse_selection(selected: &str) -> Option<(String, String)> {
    let clean = strip_ansi(selected);

    let (folder, host) = clean.split_once(':')?;

    Some((host.to_string(), folder.to_string()))
}

pub fn servers_list(ignore_hosts: Vec<String>) -> anyhow::Result<()> {
    let cache = load_or_fetch_servers_cache(ignore_hosts)?;

    let lines = build_fzf_lines(&cache);

    if lines.is_empty() {
        println!("No folders found");
        return Ok(());
    }

    lines.iter().for_each(|x| println!("{}", x));

    Ok(())
}

pub fn servers_search(config: Config) -> anyhow::Result<Option<(String, String)>> {
    let ignore_hosts = config.ignore_hosts;
    let cache = load_or_fetch_servers_cache(ignore_hosts)?;

    let lines = build_fzf_lines(&cache);

    if lines.is_empty() {
        println!("No folders found");
        return Ok(None);
    }

    if let Some(selected) = run_fzf(&lines)? {
        if let Some(selection) = parse_selection(&selected) {
            return Ok(Some(selection));
        }
    }

    Ok(None)
}

fn choose_application_command() -> Result<ApplicationCommand> {
    let options: Vec<String> = ApplicationCommand::iter()
        .map(|c| format!("{}", c))
        .collect();

    let child = Command::new("gum")
        .args(["choose", "--header", "Select application command"])
        .args(&options)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;

    if !output.status.success() {
        bail!("gum was cancelled");
    }

    let selection = String::from_utf8(output.stdout)?;
    let selection = selection.trim();

    Ok(selection.parse()?)
}

fn run_fzf(lines: &[String]) -> anyhow::Result<Option<String>> {
    let mut child = Command::new("fzf")
        .args(["--ansi", "--prompt=Select container > "])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().unwrap();
        for line in lines {
            writeln!(stdin, "{line}")?;
        }
    }

    let output = child.wait_with_output()?;

    if output.status.success() {
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(selected))
        }
    } else {
        Ok(None)
    }
}

fn load_or_fetch_servers_cache(ignore_hosts: Vec<String>) -> anyhow::Result<ServersCache> {
    let path = servers_cache_path();

    if path.exists() {
        Ok(load_servers_cache())
    } else {
        let cache = fetch_servers_cache(ignore_hosts)?;
        write_servers_cache(&cache)?;
        Ok(cache)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_dir: default_cache_dir(),
            ignore_hosts: Vec::new(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    project_dirs().cache_dir().to_path_buf()
}

fn config_dir() -> PathBuf {
    project_dirs().config_dir().to_path_buf()
}

fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

pub fn load_config() -> Config {
    let path = config_dir().join("config.toml");

    if let Ok(contents) = fs::read_to_string(&path) {
        toml::from_str(&contents).unwrap_or_default()
    } else {
        Config::default()
    }
}

fn create_and_start_spinner(message: String) -> ProgressBar {
    let style = ProgressStyle::with_template("{spinner} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);

    let bar = ProgressBar::new_spinner()
        .with_style(style)
        .with_message(message)
        .with_finish(ProgressFinish::WithMessage("✔ Done".into()));
    bar.enable_steady_tick(Duration::from_millis(100));
    bar
}

pub fn fetch_servers_cache(ignore_hosts: Vec<String>) -> anyhow::Result<ServersCache> {
    let mut hosts = read_ssh_hosts()?;
    hosts.retain(|h| !ignore_hosts.contains(h));
    let mut servers = BTreeMap::new();

    for host in hosts {
        if host.is_empty() {
            continue;
        }
        let bar = create_and_start_spinner(format!("Fetching from {host}..."));
        let folders = fetch_data_folders(&host);
        bar.finish();

        servers.insert(
            host,
            ServerEntry {
                last_updated: Utc::now().timestamp(),
                data_folders: folders,
            },
        );
    }

    Ok(ServersCache { servers })
}

pub fn write_default_config() -> anyhow::Result<()> {
    let path = config_dir().join("config.toml");
    if fs::exists(&path)? {
        bail!("Config file already exists at: {}", &path.display());
    }

    fs::create_dir_all(&config_dir())?;
    let path = config_dir().join("config.toml");
    let cfg = Config::default();
    let contents = toml::to_string_pretty(&cfg)?;
    println!("Written config to {:?}", path.display());
    std::fs::write(path, contents)?;
    Ok(())
}

pub fn init_runtime_dirs(cfg: &Config) -> anyhow::Result<()> {
    fs::create_dir_all(&cfg.cache_dir)?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServersCache {
    pub servers: BTreeMap<String, ServerEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerEntry {
    pub last_updated: i64, // unix timestamp
    pub data_folders: Vec<DataFolder>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataFolder {
    pub path: String,
    pub container: Option<String>,
}

fn fetch_data_folders(host: &str) -> Vec<DataFolder> {
    let output = Command::new("ssh")
        .arg(host)
        .arg("ls -1 /data 2>/dev/null")
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|folder| DataFolder {
                path: format!("{}", folder),
                container: None,
            })
            .collect(),
        _ => Vec::new(), // same as `|| true`
    }
}

fn fetch_containers_for_app(host: &str, app: &str) -> Result<Vec<String>> {
    let spinner = create_and_start_spinner(format!(
        "Fetching containers for host: {host} and app: {app}"
    ));
    let mut command = Command::new("ssh");
    command.arg(host).arg(format!(
        "cd /data/{app} && docker compose ps --format {{{{.Names}}}}"
    ));
    println!("{:?}", command);

    let output = command.output()?;

    spinner.finish();

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|x| x.to_owned())
        .collect())
}

pub fn load_servers_cache() -> ServersCache {
    let path = servers_cache_path();

    match fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_else(|_| empty_cache()),
        Err(_) => empty_cache(),
    }
}

fn empty_cache() -> ServersCache {
    ServersCache {
        servers: BTreeMap::new(),
    }
}

fn ensure_cache_folder() -> Result<()> {
    std::fs::create_dir_all(project_dirs().cache_dir())?;
    Ok(())
}

fn servers_cache_path() -> PathBuf {
    let cache_folder = project_dirs().cache_dir().to_path_buf();
    cache_folder.join("servers.toml")
}

pub fn write_servers_cache(cache: &ServersCache) -> anyhow::Result<()> {
    let cache_folder = project_dirs().cache_dir().to_path_buf();
    let cache_file = cache_folder.join("servers.toml");

    ensure_cache_folder()?;

    let mut tmp = NamedTempFile::new_in(&cache_folder)?;
    let contents = toml::to_string_pretty(cache)?;

    tmp.write_all(contents.as_bytes())?;
    tmp.flush()?;
    tmp.persist(&cache_file)?;

    Ok(())
}

fn read_ssh_hosts() -> anyhow::Result<Vec<String>> {
    let path = dirs::home_dir().expect("home dir").join(".ssh/config");

    let contents = fs::read_to_string(path)?;

    let hosts = contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with("Host ") {
                line.split_whitespace().nth(1).map(String::from)
            } else {
                None
            }
        })
        .collect();

    Ok(hosts)
}

fn run_container_tunnel(host: &str, app: &str, container: &str) -> Result<()> {
    let output = Command::new("./container-tunnel")
        .arg(host)
        .arg(app)
        .arg(container)
        .output()?;

    Ok(())
}

fn restore_backup_or_files(host: &str, app: &str, is_backup: bool) -> Result<()> {
    let hostpath = if is_backup {
        format!("/data/{app}/data/db/backups")
    } else {
        format!("/data/{app}/data/files/")
    };
    let localpath = if is_backup {
        format!("data/db")
    } else {
        format!("data/files")
    };
    let loading_message = if is_backup {
        "Retrieving backup files"
    } else {
        "Retrieving files"
    };
    let spinner = create_and_start_spinner(loading_message.to_owned());
    std::fs::create_dir_all(&localpath)?;
    let mut command = Command::new("rsync");
    command
        .arg("-azv")
        .arg("--partial")
        .arg("-e")
        .arg("ssh")
        .arg(format!("{host}:{hostpath}"))
        .arg(localpath)
        .output()?;
    spinner.finish();

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();
    init_runtime_dirs(&config)?;

    match cli.command {
        Commands::Servers {} => {
            let selected_server = servers_search(config)?;

            if let Some((host, app)) = selected_server {
                let command = choose_application_command()?;
                match command {
                    ApplicationCommand::Tunnel => {
                        println!("THIS IS STILL A WORK IN PROGRESS AND DOES CURRENTLY NOT WORK");
                        let containers: Vec<String> = fetch_containers_for_app(&host, &app)?;
                        let result = run_fzf(&containers)?;
                        if let Some(container) = result {
                            run_container_tunnel(&host, &app, &container)?;
                        }
                    }
                    ApplicationCommand::RestoreBackup => {
                        restore_backup_or_files(&host, &app, true)?;
                    }
                    ApplicationCommand::RestoreFiles => {
                        restore_backup_or_files(&host, &app, false)?;
                    }
                }
            } else {
                return Ok(());
            }
        }
        // Commands::Servers { command } => match command {
        //     ServersCommand::List => servers_list(config.ignore_hosts)?,
        //     ServersCommand::Tunnel => {
        //         println!("Creating server tunnel...");
        //     }
        //     ServersCommand::Refresh => {
        //         let cache = fetch_servers_cache(config.ignore_hosts)?;
        //         write_servers_cache(&cache)?;
        //     }
        //     ServersCommand::Search => {
        //         let selected_server = servers_search(config)?;

        //         if let Some((host, app)) = selected_server {
        //             let command = choose_application_command()?;
        //             match command {
        //                 ApplicationCommand::Tunnel => {
        //                     let containers = fetch_containers_for_app(&host, &app)?;
        //                     let result = run_fzf(&containers)?;
        //                     if let Some(container) = result {
        //                         run_container_tunnel(&host, &app, &container)?;
        //                     }
        //                 }
        //                 ApplicationCommand::RestoreBackup => {
        //                     restore_backup_or_files(&host, &app, true)?;
        //                 }
        //                 ApplicationCommand::RestoreFiles => {
        //                     restore_backup_or_files(&host, &app, false)?;
        //                 }
        //             }
        //         } else {
        //             return Ok(());
        //         }
        //     }
        // },
        Commands::Backups { command } => match command {
            BackupsCommand::Restore => {
                println!("Restoring backup...");
            }
        },
        Commands::Config { command } => match command {
            ConfigCommand::Init => {
                write_default_config()?;
            }
        },
    }
    Ok(())
}
