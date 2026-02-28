# Redpencil CLI
CLI tool to facilitate development at Redpencil

⚠️ Still work in progress

## Usage

```
# Creates an initial configuration file
> rpio config init

# List all remote semantic works apps that are running on the hosts configured in your SSH config.
# Uses a wizard style workflow to ask for the information it needs
> rpio apps

# You can also specify command arguments as follows to run them directly
rpio apps --host foo --app-name app-bar-qa tunnel --container-name app-bar-qa-triplestore-1 --host-port 8890 --remote-port 8890
```

Make sure you have installed:
- `fzf`
- `gum`
- `ssh`
- `rsync`
