//! Terminal output: the ✔ → ⚠ ✖ line styles, with colour only on a tty and
//! when `NO_COLOR` is unset.

use std::io::IsTerminal;
use std::sync::OnceLock;

pub struct Colors {
    pub reset: &'static str,
    pub bold: &'static str,
    pub dim: &'static str,
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub blue: &'static str,
    pub cyan: &'static str,
}

const ON: Colors = Colors {
    reset: "\x1b[0m",
    bold: "\x1b[1m",
    dim: "\x1b[2m",
    red: "\x1b[31m",
    green: "\x1b[32m",
    yellow: "\x1b[33m",
    blue: "\x1b[34m",
    cyan: "\x1b[36m",
};

const OFF: Colors = Colors {
    reset: "",
    bold: "",
    dim: "",
    red: "",
    green: "",
    yellow: "",
    blue: "",
    cyan: "",
};

/// The active colour palette (all empty strings when colour is disabled).
pub fn c() -> &'static Colors {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    let enabled = *ENABLED
        .get_or_init(|| std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none());
    if enabled {
        &ON
    } else {
        &OFF
    }
}

pub fn ok(msg: &str) {
    let c = c();
    println!("  {}✔{}  {msg}", c.green, c.reset);
}

pub fn info(msg: &str) {
    let c = c();
    println!("  {}→{}  {msg}", c.blue, c.reset);
}

pub fn warn(msg: &str) {
    let c = c();
    println!("  {}⚠{}  {msg}", c.yellow, c.reset);
}

pub fn err(msg: &str) {
    let c = c();
    eprintln!("  {}✖{}  {msg}", c.red, c.reset);
}

pub fn dim(msg: &str) {
    let c = c();
    println!("     {}{msg}{}", c.dim, c.reset);
}

pub fn head(msg: &str) {
    let c = c();
    println!("\n{}{}{msg}{}", c.bold, c.cyan, c.reset);
}

/// Ask a yes/no question on stdin; anything but y/yes (including EOF) is no.
pub fn confirm(prompt: &str) -> bool {
    use std::io::Write;
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}
