use chrono::Local;
use clap::Parser;
use env_logger::{Builder, Env};
use std::io::Write;

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Path(s) to search for files within.
    pub root: Vec<String>,
    /// Should plan be executed
    #[clap(long)]
    pub execute: bool,
    /// Path to use for temporary storage
    #[clap(long)]
    pub scratch: Vec<String>,
}

pub fn setup_logger(is_test: bool) {
    let env = Env::default().filter_or("RUST_LOG", "info");
    Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .is_test(is_test)
        .init();
}

mod filesystem;
mod state;

pub use state::{Entry, Move, State};
