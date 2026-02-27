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
use serde_yaml::Value;
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
    Apps {
        #[arg(long, num_args = 0..)]
        refresh: Option<Vec<String>>,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum AppsCommand {
    List,
    Refresh,
    Search,
    Tunnel,
}

// An operation on a single application
#[derive(Debug, Clone, Copy, EnumIter, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
enum ApplicationCommand {
    SshSession,
    Tunnel,
    RetrieveBackup,
    RetrieveFiles,
    HostedUrl,
}

#[derive(Subcommand)]
enum BackupsCommand {
    Restore,
}

#[derive(Subcommand)]
enum ConfigCommand {
    Init,
}

pub struct RemoteApp {
    host: String,
    app_name: String,
}

impl RemoteApp {
    fn new(host: String, app_name: String) -> Self {
        RemoteApp { host, app_name }
    }

    fn fetch_containers(&self) -> Result<Vec<String>> {
        let spinner = create_and_start_spinner(&format!(
            "Fetching containers for host: {} and app: {}",
            &self.host, &self.app_name
        ));
        let mut command = Command::new("ssh");
        command.arg(&self.host).arg(format!(
            "cd /data/{} && docker compose ps --format {{{{.Names}}}}",
            &self.app_name
        ));
        println!("{:?}", command);

        let output = command.output()?;

        spinner.finish();

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|x| x.to_owned())
            .collect())
    }

    fn retrieve_app_docker_config(&self) -> Result<String> {
        let spinner =
            create_and_start_spinner(&format!("Fetching docker config for {}", &self.app_name));
        let output = Command::new("ssh")
            .arg(&self.host)
            .arg(format!(
                "cd {} && docker compose config",
                self.remote_directory()
            ))
            .output()?;

        spinner.finish();

        Ok(String::from_utf8(output.stdout)?)
    }

    fn remote_directory(&self) -> String {
        format!("/data/{}", self.app_name)
    }
}

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("com", "redpencil", "rpio-cli").expect("Could not determine config directory")
}

fn get_env(doc: &Value, service: &str, key: &str) -> Option<String> {
    let services = doc.get("services")?;
    let svc = services.get(service)?;
    let env = svc.get("environment")?;

    match env {
        Value::Mapping(map) => map
            .get(&Value::String(key.into()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        Value::Sequence(seq) => seq.iter().filter_map(|v| v.as_str()).find_map(|entry| {
            let (k, v) = entry.split_once('=')?;
            if k == key { Some(v.to_string()) } else { None }
        }),
        _ => None,
    }
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
    let cache = load_or_fetch_servers_cache(&ignore_hosts)?;

    let lines = build_fzf_lines(&cache);

    if lines.is_empty() {
        println!("No folders found");
        return Ok(());
    }

    lines.iter().for_each(|x| println!("{}", x));

    Ok(())
}

pub fn servers_search(config: &Config) -> anyhow::Result<Option<RemoteApp>> {
    let ignore_hosts = &config.ignore_hosts;
    let cache = load_or_fetch_servers_cache(&ignore_hosts)?;

    let lines = build_fzf_lines(&cache);

    if lines.is_empty() {
        println!("No folders found");
        return Ok(None);
    }

    if let Some(selected) = run_fzf(&lines)? {
        if let Some((host, app_name)) = parse_selection(&selected) {
            return Ok(Some(RemoteApp::new(host, app_name)));
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

    let selection = String::from_utf8(output.stdout)?.trim().to_owned();

    Ok(selection.parse()?)
}

fn run_fzf(lines: &[String]) -> anyhow::Result<Option<String>> {
    let mut child = Command::new("fzf")
        .args(["--ansi", "--prompt=Select application > "])
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

fn load_or_fetch_servers_cache(ignore_hosts: &Vec<String>) -> anyhow::Result<ServersCache> {
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

fn create_and_start_spinner(message: &str) -> ProgressBar {
    let style = ProgressStyle::with_template("{spinner} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);

    let bar = ProgressBar::new_spinner()
        .with_style(style)
        .with_message(message.to_owned())
        .with_finish(ProgressFinish::WithMessage("✔ Done".into()));
    bar.enable_steady_tick(Duration::from_millis(100));
    bar
}

pub fn fetch_servers_cache(ignore_hosts: &Vec<String>) -> anyhow::Result<ServersCache> {
    let mut hosts = read_ssh_hosts()?;
    hosts.retain(|h| !ignore_hosts.contains(h));
    let mut servers = BTreeMap::new();

    for host in hosts {
        if host.is_empty() {
            continue;
        }
        let bar = create_and_start_spinner(&format!("Indexing apps from {host}..."));
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

fn prompt_number(prompt: &str) -> Result<u32> {
    let output = Command::new("gum")
        .arg("input")
        .arg("--placeholder")
        .arg("Enter a number...")
        .arg("--header")
        .arg(&prompt)
        .stderr(Stdio::inherit())
        .output()?;

    if !output.status.success() {
        bail!("gum was cancelled");
    }

    let input_str = String::from_utf8_lossy(&output.stdout).trim().to_owned();

    Ok(input_str.parse::<u32>()?)
}

// Improve this with a ssh crate?
fn run_container_tunnel(host: &str, app: &str, container: &str) -> Result<()> {
    let mut container_ip_command: Command = Command::new("ssh");
    let output = container_ip_command
        .arg(&host)
        .arg(format!("docker inspect -f '{{{{range .NetworkSettings.Networks}}}}{{{{println .IPAddress}}}}{{{{end}}}}' {container} | head -n1"))
        .output()?;

    let output_chars = String::from_utf8_lossy(&output.stdout);
    let container_ip = output_chars.trim();
    let remote_port = prompt_number("Choose a port on the container")?;
    let host_port: u32 = prompt_number("What local port to use?")?;

    let status = Command::new("ssh")
        .arg(host)
        .arg("-L")
        .arg(format!("{host_port}:{container_ip}:{remote_port}"))
        .arg("-N")
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-o")
        .arg("ServerAliveInterval=60")
        .spawn()?;

    println!("Opening tunnel on http://localhost:{host_port}");
    println!("Press Ctrl+C to exit");

    status.wait_with_output()?;

    Ok(())
}

fn restore_backup_or_files(
    host: &str,
    app: &str,
    sw_root_folder: &PathBuf,
    is_backup: bool,
) -> Result<()> {
    let hostpath = if is_backup {
        format!("/data/{app}/data/db/backups")
    } else {
        format!("/data/{app}/data/files/")
    };
    let mut localpath = sw_root_folder.to_owned();
    localpath.push(if is_backup { "data/db" } else { "data/files" });

    let loading_message = if is_backup {
        "Retrieving backup files"
    } else {
        "Retrieving files"
    };
    let spinner = create_and_start_spinner(&loading_message);
    std::fs::create_dir_all(&localpath)?;
    let mut command = Command::new("rsync");
    command
        .arg("-azv")
        .arg("--partial")
        .arg("-e")
        .arg("ssh")
        .arg(format!("{host}:{hostpath}"))
        .arg(localpath);
    println!("{:?}", command);
    let output = command.output()?;
    spinner.finish();
    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "rsync failed with status {}: {}",
            output.status,
            error_message
        ));
    }

    Ok(())
}

fn attach_ssh_session(remote_app: &RemoteApp) -> Result<()> {
    let app_dir = directory_for_app(&remote_app.app_name);
    let mut command = Command::new("ssh");
    command
        .arg("-t")
        .arg(&remote_app.host)
        .arg(format!("cd {app_dir} ; bash --login"));
    println!("Running command {:?}", command);
    command.status()?;

    Ok(())
}

fn directory_for_app(app: &str) -> String {
    format!("/data/{app}")
}

fn find_semantic_works_root_folder() -> Result<PathBuf> {
    let mut current_dir = std::env::current_dir()?;

    loop {
        let compose_file = current_dir.join("docker-compose.yml");

        if compose_file.exists() {
            let contents = fs::read_to_string(&compose_file)?;
            let doc: Value = serde_yaml::from_str(&contents)?;

            if let Some(services) = doc.get("services").and_then(|s| s.as_mapping()) {
                for (_name, service) in services {
                    if let Some(image) = service.get("image").and_then(|v| v.as_str()) {
                        if image.starts_with("semtech/mu-identifier") {
                            return Ok(current_dir);
                        }
                    }
                }
            }
        }

        // Go up one directory
        if !current_dir.pop() {
            break;
        }
    }

    bail!(
        "Could not find docker-compose.yml with service using image semtech/mu-identifier in this or any parent directory"
    );
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();
    init_runtime_dirs(&config)?;

    match cli.command {
        Commands::Apps { refresh } => {
            match refresh {
                None => {}
                Some(servers) if servers.is_empty() => {
                    let cache = fetch_servers_cache(&config.ignore_hosts)?;
                    write_servers_cache(&cache)?;
                }
                Some(servers) => {
                    bail!("Partial refresh not implemented yet!");
                }
            }
            let selected_server = servers_search(&config)?;

            if let Some(remote_app) = selected_server {
                let command = choose_application_command()?;
                match command {
                    ApplicationCommand::Tunnel => {
                        println!("THIS IS STILL A WORK IN PROGRESS AND DOES CURRENTLY NOT WORK");
                        let containers: Vec<String> = remote_app.fetch_containers()?;
                        let result = run_fzf(&containers)?;
                        if let Some(container) = result {
                            run_container_tunnel(
                                &remote_app.host,
                                &remote_app.app_name,
                                &container,
                            )?;
                        }
                    }
                    ApplicationCommand::SshSession => {
                        attach_ssh_session(&remote_app)?;
                    }
                    ApplicationCommand::RetrieveBackup => {
                        let root_folder = find_semantic_works_root_folder()?;
                        restore_backup_or_files(
                            &remote_app.host,
                            &remote_app.app_name,
                            &root_folder,
                            true,
                        )?;
                    }
                    ApplicationCommand::RetrieveFiles => {
                        let root_folder = find_semantic_works_root_folder()?;
                        restore_backup_or_files(
                            &remote_app.host,
                            &remote_app.app_name,
                            &root_folder,
                            false,
                        )?;
                    }
                    ApplicationCommand::HostedUrl => {
                        let yaml = remote_app.retrieve_app_docker_config()?;
                        let doc: Value = serde_yaml::from_str(&yaml)?;
                        if let Some(url) = get_env(&doc, "identifier", "LETSENCRYPT_HOST") {
                            println!("https://{url}");
                        }
                    }
                }
            } else {
                return Ok(());
            }
        }
        Commands::Config { command } => match command {
            ConfigCommand::Init => {
                write_default_config()?;
            }
        },
    }
    Ok(())
}
