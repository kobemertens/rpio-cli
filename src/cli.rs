use clap::{Parser, Subcommand};
use strum_macros::{Display, EnumIter, EnumString};

#[derive(Parser)]
#[command(name = "rpm")]
#[command(about = "RPM CLI tool", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: CommandsCli,
}

#[derive(Subcommand, Clone)]
pub enum CommandsCli {
    Apps {
        #[arg(short, long)]
        refresh: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        app_name: Option<String>,
        #[command(subcommand)]
        app_command: Option<ApplicationCommandCli>,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Clone, EnumIter, EnumString, Display, Subcommand)]
#[strum(serialize_all = "kebab-case")]
pub enum ApplicationCommandCli {
    SshSession,
    Tunnel {
        #[arg(long)]
        container_name: Option<String>,
        #[arg(long)]
        host_port: Option<u32>,
        #[arg(long)]
        remote_port: Option<u32>,
    },
    RetrieveBackup,
    RetrieveFiles,
    HostedUrl,
}

#[derive(Subcommand, Clone)]
pub enum ConfigCommand {
    Init,
}
