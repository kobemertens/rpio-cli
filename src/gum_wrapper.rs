use anyhow::{Result, bail};
use std::process::{Command, Stdio};

pub fn prompt_number(prompt: &str) -> Result<u32> {
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
