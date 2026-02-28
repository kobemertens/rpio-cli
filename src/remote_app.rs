use anyhow::Result;
use anyhow::anyhow;
use std::process::Command;
use std::str::FromStr;
use crate::spinner::create_and_start_spinner;

#[derive(Clone)]
pub struct RemoteApp {
    pub host: String,
    pub app_name: String,
}

impl RemoteApp {
    pub fn new(host: String, app_name: String) -> Self {
        RemoteApp { host, app_name }
    }

    pub fn fetch_containers(&self) -> Result<Vec<String>> {
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

    pub fn retrieve_app_docker_config(&self) -> Result<String> {
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

impl FromStr for RemoteApp {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (app_name, host) = s
            .split_once(':')
            .ok_or_else(|| anyhow!("Invalid format '{}': expected 'host:app_name'", s))?;

        Ok(RemoteApp {
            host: host.to_string(),
            app_name: app_name.to_string(),
        })
    }
}
