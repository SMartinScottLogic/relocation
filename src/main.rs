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
use std::path::PathBuf;

#[derive(Clone, Debug)]
struct SizedFile {
    filename: String,
    size: u64
}

fn all_paths(base: &std::ffi::OsString, path: &PathBuf) -> Vec<PathBuf> {
    path.strip_prefix(base).unwrap().iter().fold((Vec::new(), PathBuf::from("")), |mut acc, component| {
        acc.1.push(component);
        acc.0.push(acc.1.clone());
        acc
    }).0
}

fn find_files(sourceroot: &std::ffi::OsString) -> BTreeMap<PathBuf, u64> {
    info!("find all files in {:?}.", sourceroot);

    let walk = WalkDir::new(sourceroot).into_iter();

    walk
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_file())
        .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_str().unwrap().to_string()))
        .filter(|(size, _filename)| size > &0u64)
        .map(|(size, filename)| (size, all_paths(sourceroot, &PathBuf::from(filename).parent().unwrap().to_path_buf())))
        .fold(BTreeMap::new(), |mut acc, entry| {
            let size = entry.0;
            let paths = entry.1;
            for path in paths {
            *acc.entry(path).or_insert(0) += size;
            }
            acc
        })
        /*
        .flat_map(|(size, filename)| all_paths(sourceroot, &PathBuf::from(filename)).iter().map(move |path| (size, path)))
        .fold(BTreeMap::new(), |mut acc, entry| {
            let size = entry.0;
            let path = entry.1;
            *acc.entry(std::ffi::OsString::from(path)).or_insert(0) += size;
            acc
        })
        */
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

    let sourceroot = env::args_os().nth(1).unwrap_or_else(|| std::ffi::OsString::from("."));
    let groups = find_files(&sourceroot);

    info!("result: {:#?}", groups);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    
    #[test]
    fn all_paths() {
        assert_eq!(
            super::all_paths(
                &std::ffi::OsString::from("/workspace"), 
                &PathBuf::from("/workspace/relocation/.target/test/1234")
            ), 
            vec!["relocation", "relocation/.target", "relocation/.target/test", "relocation/.target/test/1234"]
            .iter().map(PathBuf::from).collect::<Vec<PathBuf>>());
    }
}