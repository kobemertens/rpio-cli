use std::io::Write;
use std::process::{Command, Stdio};

pub fn run_fzf(lines: &[String], prompt: &str) -> anyhow::Result<Option<String>> {
    let mut child = Command::new("fzf")
        .args(["--ansi", &format!("--prompt={prompt} > ")])
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
