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
use std::ffi::OsString;

#[derive(Clone, Debug)]
struct SizedFile {
    filename: String,
    size: u64
}

fn all_paths(base: &OsString, path: &PathBuf) -> Vec<PathBuf> {
    path.strip_prefix(base).unwrap().iter().fold((Vec::new(), PathBuf::from("")), |mut acc, component| {
        acc.1.push(component);
        acc.0.push(acc.1.clone());
        acc
    }).0
}

fn find_files(sourceroot: &OsString) -> BTreeMap<PathBuf, u64> {
    info!("find all files in {:?}.", sourceroot);

    let walk = WalkDir::new(sourceroot).into_iter();

    walk
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| (entry.metadata().unwrap().len(), entry.path().to_str().unwrap().to_string()))
        .filter(|(size, _filename)| size > &0u64)
        .map(|(size, filename)| (size, all_paths(sourceroot, &PathBuf::from(filename).parent().unwrap().to_path_buf())))
        .fold(BTreeMap::new(), |mut acc, entry| {
            let (size, paths) = entry;
            for path in paths {
                *acc.entry(path).or_insert(0) += size;
            }
            acc
        })
}

fn merge_files(all_files: &mut BTreeMap<PathBuf, (u64, BTreeMap<OsString, u64>)>, sourceroot: &OsString, files: BTreeMap<PathBuf, u64>) {
    for (path, size) in files {
        let entry = all_files.entry(path).or_insert((0, BTreeMap::new()));
        if !entry.1.contains_key(sourceroot) {
            entry.0 += size;
            entry.1.insert(sourceroot.clone(), size);
        }
    };
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
    let mut all_files = BTreeMap::new();

    for arg in env::args_os().skip(1) {
        let groups = find_files(&arg);
        debug!("result: {:#?}", groups);
        merge_files(&mut all_files, &arg, groups);
    }
    debug!("result: {:?}", all_files);
}

#[cfg(test)]
#[macro_use]
extern crate maplit;

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, collections::BTreeMap, ffi::OsString};
    
    #[test]
    fn all_paths() {
        assert_eq!(
            super::all_paths(
                &OsString::from("/workspace"), 
                &PathBuf::from("/workspace/relocation/.target/test/1234")
            ), 
            vec!["relocation", "relocation/.target", "relocation/.target/test", "relocation/.target/test/1234"]
            .iter().map(PathBuf::from).collect::<Vec<PathBuf>>());
    }

    #[test]
    fn merge_files() {
        let mut all_files = BTreeMap::new();
        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("path1"), 1);
        files.insert(PathBuf::from("path2"), 10);
        files.insert(PathBuf::from("path2/A"), 7);
        files.insert(PathBuf::from("path2/B"), 3);
        super::merge_files(&mut all_files, &OsString::from("."), files);
        println!("{:?}", all_files);
        assert_eq!(all_files.get(&PathBuf::from("path1")).unwrap(), &(1u64, btreemap!{OsString::from(".") => 1u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2")).unwrap(), &(10u64, btreemap!{OsString::from(".") => 10u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2/A")).unwrap(), &(7u64, btreemap!{OsString::from(".") => 7u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2/B")).unwrap(), &(3u64, btreemap!{OsString::from(".") => 3u64}));
    }
   
    #[test]
    fn merge_files_second_source() {
        let mut all_files = BTreeMap::new();
        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("path1"), 1);
        files.insert(PathBuf::from("path2"), 10);
        super::merge_files(&mut all_files, &OsString::from("."), files.clone());
        files.clear();
        files.insert(PathBuf::from("path1"), 11);
        super::merge_files(&mut all_files, &OsString::from("other"), files.clone());
        println!("{:?}", all_files);
        assert_eq!(all_files.get(&PathBuf::from("path1")).unwrap(), &(12u64, btreemap!{OsString::from(".") => 1u64, OsString::from("other") => 11u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2")).unwrap(), &(10u64, btreemap!{OsString::from(".") => 10u64}));
    }
    
    #[test]
    fn merge_files_twice() {
        let mut all_files = BTreeMap::new();
        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("path1"), 1);
        files.insert(PathBuf::from("path2"), 10);
        files.insert(PathBuf::from("path2/A"), 7);
        files.insert(PathBuf::from("path2/B"), 3);
        super::merge_files(&mut all_files, &OsString::from("."), files.clone());
        super::merge_files(&mut all_files, &OsString::from("."), files.clone());
        println!("{:?}", all_files);
        assert_eq!(all_files.get(&PathBuf::from("path1")).unwrap(), &(1u64, btreemap!{OsString::from(".") => 1u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2")).unwrap(), &(10u64, btreemap!{OsString::from(".") => 10u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2/A")).unwrap(), &(7u64, btreemap!{OsString::from(".") => 7u64}));
        assert_eq!(all_files.get(&PathBuf::from("path2/B")).unwrap(), &(3u64, btreemap!{OsString::from(".") => 3u64}));
    }
}