# Redpencil CLI
CLI tool to facilitate development at Redpencil

## Usage
```sh
$ rpio
```

#### (Optional) Generate config file
```sh
# Creates an initial configuration file
$ rpio config init
```

```sh
# List all remote semantic works apps that are running on the hosts configured in your SSH config.
# Uses a wizard style workflow to ask for the information it needs
$ rpio apps
```

# You can also specify command arguments as follows to run them directly. 
# Any missing required arguments will be prompted.
$ rpio apps --host foo --app-name app-bar-qa tunnel --container-name app-bar-qa-triplestore-1 --host-port 8890 --remote-port 8890
```

## Installation

### Prerequisites
Make sure you have these installed and available on your `PATH`, otherwise the application will not work correctly.
- `fzf`
- `gum`
- `ssh`
- `rsync`

### Build instructions
Install Rust and cargo using your preferred method (or have a look [here](https://rust-lang.org/tools/install/)).
```sh
git clone git@github.com:kobemertens/rpio-cli.git
cd rpio-cli
cargo build --release
```


This will create a executable `target/release/rpio` that you can add to your `PATH`.

## Docker
⚠️ Still work in progress, feel free to contribute.


