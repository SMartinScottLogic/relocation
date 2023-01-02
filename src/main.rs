extern crate env_logger;
extern crate log;

use clap::StructOpt;
use log::debug;
use relocation::{NewState, OverlayState, StateNames};

use relocation::{setup_logger, Config};

fn main() -> Result<(), std::io::Error> {
    let config = Config::parse();

    setup_logger(false);

    let mut names = StateNames::default();
    let mut initial = OverlayState::default();
    for root in &config.root {
        initial.scan(&mut names, root, false);
    }
    for scratch in &config.scratch {
        initial.scan(&mut names, scratch, true);
    }

    debug!("initially: {initial:#?}, names: {names:?}");

    let (moves, _cost) = initial.relocate().unwrap_or_default();
    if config.execute {
        for m in moves {
            println!("Move {:?}", m);
        }
    }
    Ok(())
}
