extern crate env_logger;
extern crate log;

use clap::StructOpt;
use log::debug;
use relocation::State;

use relocation::{setup_logger, Config};

fn main() -> Result<(), std::io::Error> {
    let config = Config::parse();

    setup_logger(false);

    let mut initial = State::default();
    for root in &config.root {
        initial += root;
    }
    for scratch in &config.scratch {
        initial.scan(scratch, true);
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
