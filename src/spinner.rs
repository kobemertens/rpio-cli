use indicatif::ProgressBar;
use indicatif::ProgressFinish;
use indicatif::ProgressStyle;
use std::time::Duration;

pub fn create_and_start_spinner(message: &str) -> ProgressBar {
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
