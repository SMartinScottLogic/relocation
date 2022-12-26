extern crate env_logger;
extern crate log;

use chrono::Local;
use clap::StructOpt;
use log::{error, info, debug};
use pathfinding::prelude::idastar;
use relocation::State;
use std::io::Write;

use env_logger::{Builder, Env};

use relocation::Config;

fn setup_logger() {
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
        .init();
}

fn main() -> Result<(), std::io::Error> {
    let config = Config::parse();

    setup_logger();

    let mut initial = State::default();
    for root in &config.root {
        initial += root;
    }

    debug!("initially: {initial:#?}");

    let (moves, _cost) = initial.relocate().unwrap_or_default();

    if config.execute {
        for m in moves {
            println!("Move {:?} to {:?}", m.source, m.target);
        }
    }
    Ok(())
}
