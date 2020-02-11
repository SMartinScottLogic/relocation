#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;

use std::env;
use walkdir::WalkDir;
use std::collections::BTreeMap;
use std::io::prelude::*;
use env_logger::{Builder,Env};
use chrono::Local;

#[derive(Clone, Debug)]
struct SizedFile {
    filename: String,
    size: u64
}

fn find_files(sourceroot: std::ffi::OsString) -> Vec<Vec<SizedFile>> {
    info!("find all files in {:?}.", sourceroot);

    let walk = WalkDir::new(sourceroot).into_iter();

    walk
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_file())
        .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_str().unwrap().to_string()))
        .fold(BTreeMap::new(), |mut acc, entry| {
            let size = entry.0;
            acc.entry(size).or_insert_with(Vec::new).push(SizedFile {filename: entry.1, size});
            acc
        }).values().cloned().collect()
}

fn main() {
    let env = Env::default()
        .filter_or("RUST_LOG", "info");
    Builder::from_env(env)
        .format(|buf, record| {
            writeln!(buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();
    // TODO cmd-line args

    let sourceroot = env::args_os().nth(1).unwrap();
    let groups = find_files(sourceroot);

    info!("result: {:#?}", groups);
}
