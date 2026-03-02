use clap::{Parser, Subcommand};
use strum_macros::{Display, EnumIter, EnumString};

#[derive(Parser)]
#[command(name = "rpio")]
#[command(about = "Redpencil CLI tool", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: CommandsCli,
}

#[derive(Subcommand, Clone)]
pub enum CommandsCli {
    #[command(about = "Manage deployed applications")]
    Apps {
        #[arg(short, long, help="Re-index all hosts configured in your ssh config")]
        refresh: bool,
        #[arg(long, help="Not implemented yet")]
        dry_run: bool,
        #[arg(long, help="Server where the app is hosted")]
        host: Option<String>,
        #[arg(long, help="Name of the hosted application")]
        app_name: Option<String>,
        #[command(subcommand)]
        app_command: Option<ApplicationCommandCli>,
    },
    #[command(about = "Manage configuration")]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Clone, EnumIter, EnumString, Display, Subcommand)]
#[strum(serialize_all = "kebab-case")]
pub enum ApplicationCommandCli {
    #[command(about="Start an interactive ssh session to the specified app")]
    SshSession,
    #[command(about="Open a ssh tunnel to the specified app")]
    Tunnel {
        #[arg(long)]
        container_name: Option<String>,
        #[arg(long)]
        host_port: Option<u32>,
        #[arg(long)]
        remote_port: Option<u32>,
    },
    #[command(about="Copy all backup files from the specified remote app to your local app")]
    RetrieveBackup,
    #[command(about="Copy all files from the specified remote app to your local app")]
    RetrieveFiles,
    #[command(about="Retrieve and display the URL where the app is hosted")]
    HostedUrl,
}

#[derive(Subcommand, Clone)]
pub enum ConfigCommand {
    #[command(about = "Create initial configuration file")]
    Init,
}
