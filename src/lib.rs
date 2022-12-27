use clap::Parser;
use log::{debug, error, info, trace};
use pathfinding::prelude::idastar;
use std::{
    collections::{HashMap, HashSet},
    convert::From,
    ops::AddAssign,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Path(s) to search for files within.
    pub root: Vec<String>,
    /// Should plan be executed
    #[clap(long)]
    pub execute: bool,
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct State {
    roots: HashMap<std::path::PathBuf, (u64, u64, u64)>,
    entries: Vec<Entry>,
    usage: HashMap<PathBuf, HashMap<PathBuf, u64>>,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Entry {
    size: u64,
    root: PathBuf,
    subdir: PathBuf,
    subpath: PathBuf,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Move {
    pub source: PathBuf,
    pub target: PathBuf,
}

impl State {
    pub fn moves(&self) -> Vec<Move> {
        //self.moves.clone()
        println!("moves from {self:#?}");
        Vec::new()
    }
}

impl State {
    pub fn relocate(&self) -> Option<(Vec<Move>, u64)> {
        let r = idastar(self, |s| s.successors(), |s| s.heuristic(), |s| s.success());
        if r.is_none() {
            error!("No complete relocation found. Possibly try each subdir in turn.");
            return None;
        }
        let (states, cost) = r.unwrap();
        if states.len() == 1 {
            info!("Already fully relocated");
            return None;
        }
        let moves = Self::calculate_moves(&states);
        info!(
            "Relocation solution found, with {} moves, costing {}: {:?}",
            moves.len(),
            cost,
            moves
        );
        Some((moves, cost))
    }

    fn calculate_moves(states: &[State]) -> Vec<Move> {
        let it1 = states.iter().skip(1);
        states
            .iter()
            .zip(it1)
            .map(|(a, b)| {
                let src = a
                    .entries
                    .iter()
                    .map(|e| e.root.join(&e.subdir).join(&e.subpath))
                    .collect::<HashSet<_>>();
                let tgt = b
                    .entries
                    .iter()
                    .map(|e| e.root.join(&e.subdir).join(&e.subpath))
                    .collect::<HashSet<_>>();
                let only_src = src.difference(&tgt).next().unwrap();
                let only_tgt = tgt.difference(&src).next().unwrap();
                Move {
                    source: only_src.clone(),
                    target: only_tgt.clone(),
                }
            })
            .collect::<Vec<_>>()
    }
}

impl State {
    fn successors(&self) -> Box<dyn Iterator<Item = (State, u64)>> {
        if self.entries.len() > 100000 {
            // TODO Lazy resolution of successors
            panic!("Too many entries");
            // let successors = LazySuccessors::new(self);
            // Box::new(successors)
        } else {
            let mut num_tests = 0_u64;
            let mut result = Vec::new();
            for entry in &self.entries {
                for (other_root, (_other_fsid, other_block_size, other_blocks_avail)) in &self.roots
                {
                    if entry.root == *other_root {
                        continue;
                    }
                    num_tests += 1;
                    if other_block_size.saturating_mul(*other_blocks_avail) < entry.size {
                        // No space to move into 'other_root' at this point
                        debug!(
                            "{}: No space for {:?} in {:?}",
                            num_tests, entry, other_root
                        );
                        continue;
                    }
                    let mut new_entries = self
                        .entries
                        .iter()
                        .filter(|e| *e != entry)
                        .cloned()
                        .collect::<Vec<_>>();
                    let mut new_entry = entry.clone();
                    new_entry.root = other_root.clone();
                    new_entries.push(new_entry);

                    let cost = other_block_size.saturating_mul(1 + (entry.size / other_block_size));
                    debug!(
                        "{}: Candidate: move {:?} to {:?} (cost {} = {} * (1 + ({} / {})))",
                        num_tests,
                        entry.root.join(&entry.subdir).join(entry.subpath.clone()),
                        other_root.join(&entry.subdir).join(entry.subpath.clone()),
                        cost,
                        other_block_size,
                        entry.size,
                        other_block_size
                    );
                    // Modify free blocks in roots
                    let mut roots = self.roots.clone();
                    roots.entry(entry.root.clone()).and_modify(
                        |(_cur_fsid, cur_block_size, cur_blocks_avail)| {
                            // Pessimistic freeing of blocks
                            *cur_blocks_avail +=
                                cur_block_size.saturating_mul(entry.size / *cur_block_size);
                        },
                    );
                    roots.entry(other_root.clone()).and_modify(
                        |(_other_fsid, other_block_size, other_blocks_avail)| {
                            // Pessimistic consumption of blocks
                            *other_blocks_avail -= other_block_size
                                .saturating_mul(1 + (entry.size / *other_block_size));
                        },
                    );
                    // Modify usage
                    let mut usage = self.usage.clone();
                    *usage
                        .entry(entry.subdir.clone())
                        .or_default()
                        .entry(entry.root.clone())
                        .or_default() -= 1;
                    *usage
                        .entry(entry.subdir.clone())
                        .or_default()
                        .entry(other_root.clone())
                        .or_default() += 1;
                    // Add new state to results
                    let new_state = State {
                        entries: new_entries,
                        roots,
                        usage,
                    };
                    result.push((new_state, cost));
                }
            }
            debug!("successors to {self:?}:");
            for r in &result {
                debug!("    {r:#?}");
            }
            Box::new(ExistingSuccessors::from(result))
        }
    }

    fn heuristic(&self) -> u64 {
        let mut total = 0;
        for subdir in self.usage.keys() {
            let v = self.entries.iter().filter(|e| e.subdir == *subdir).fold(
                HashMap::new(),
                |mut acc, entry| {
                    *acc.entry(&entry.root).or_insert(0_u64) += entry.size;
                    acc
                },
            );
            if v.is_empty() {
                info!("entries: {:?}", self.entries);
                info!("usage: {:?}", self.usage);
                info!("subpath {:?}, values = {:?}", subdir, v);
            }
            // Total size of all files within this subpath (over all roots)
            let subpath_total: u64 = v.values().sum();
            // Minimum cost of moving all files to each root (total within that root, less the overall total)
            let min_cost = v.values().map(|v| subpath_total - *v).min().unwrap();
            total += min_cost;
        }
        total
    }

    fn success(&self) -> bool {
        !self
            .usage
            .iter()
            .any(|(_subpath, roots)| roots.values().filter(|v| **v != 0).count() > 1)
    }
}

impl AddAssign<&str> for State {
    fn add_assign(&mut self, rhs: &str) {
        self.scan(rhs);
    }
}

impl AddAssign<String> for State {
    fn add_assign(&mut self, rhs: String) {
        self.scan(&rhs);
    }
}

impl AddAssign<&String> for State {
    fn add_assign(&mut self, rhs: &String) {
        self.scan(rhs);
    }
}

impl State {
    fn to_cpath(path: &std::path::Path) -> Vec<u8> {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

        let path_os: &OsStr = path.as_ref();
        let mut cpath = path_os.as_bytes().to_vec();
        cpath.push(0);
        cpath
    }

    fn stats(mount_point: &Path) -> Option<(u64, u64, u64)> {
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            let mount_point_cpath = Self::to_cpath(mount_point);
            if libc::statvfs(mount_point_cpath.as_ptr() as *const _, &mut stat) == 0 {
                Some((stat.f_fsid, stat.f_bsize, stat.f_bavail))
            } else {
                None
            }
        }
    }

    fn add_entry(&mut self, root: PathBuf, subdir: PathBuf, subpath: PathBuf, size: u64) {
        *self
            .usage
            .entry(subdir.clone())
            .or_default()
            .entry(root.clone())
            .or_default() += 1;
        let entry = Entry {
            root,
            subdir,
            subpath,
            size,
        };
        self.entries.push(entry);
    }

    fn scan(&mut self, root: &str) {
        let cur_dir = std::env::current_dir().unwrap();
        let root = cur_dir.join(root).canonicalize().unwrap();
        if self.roots.contains_key(&root) {
            error!("Skipping duplicate scan root: {root:?}");
            return;
        }
        let root_dev_id = root.metadata().unwrap().dev();
        let (fsid, bsize, bavail) = Self::stats(&root).unwrap();
        self.roots.insert(root.clone(), (fsid, bsize, bavail));
        info!(
            "scan {:?} from {}",
            root,
            std::env::current_dir().unwrap().display()
        );
        let walker = WalkDir::new(&root).same_file_system(true).into_iter();
        for entry in walker.flatten() {
            let metadata = entry.metadata().unwrap();
            let dev_id = metadata.dev();

            trace!(
                "{:?} {} {:o} {:?} {} {} (@ {})",
                dev_id,
                entry.path().display(),
                metadata.mode(),
                metadata.is_dir(),
                metadata.is_file(),
                metadata.size(),
                std::env::current_dir()
                    .unwrap()
                    .as_path()
                    .join(entry.path())
                    .canonicalize()
                    .map_or("[missing]".to_string(), |p| p.display().to_string())
            );

            if dev_id != root_dev_id {
                error!(
                    "skipping {}: not on same device as origin root {:?}",
                    entry.path().display(),
                    root.clone()
                );
                continue;
            }

            if !metadata.is_file() {
                debug!("skipping {}: not a file", entry.path().display());
                continue;
            }
            let c = entry
                .path()
                .strip_prefix(&root)
                .unwrap()
                .components()
                .next()
                .unwrap();
            let subdir = if entry
                .path()
                .strip_prefix(root.join(c))
                .unwrap()
                .iter()
                .count()
                == 0
            {
                PathBuf::new()
            } else {
                PathBuf::from(c.as_os_str())
            };
            let subpath = entry.path().strip_prefix(root.join(&subdir)).unwrap();
            let subpath = subpath.to_path_buf();
            debug!(
                "{:?} {:?} {:?} {:?} {:o} {:?} {} {} (@ {})",
                dev_id,
                root,
                subdir,
                subpath,
                metadata.mode(),
                metadata.is_dir(),
                metadata.is_file(),
                metadata.size(),
                cur_dir
                    .join(entry.path())
                    .canonicalize()
                    .map_or("[missing]".to_string(), |p| p.display().to_string())
            );
            let size = metadata.size();
            self.add_entry(root.clone(), subdir, subpath, size);
        }
    }
}

struct ExistingSuccessors {
    existing: std::vec::IntoIter<(State, u64)>,
}
impl Iterator for ExistingSuccessors {
    type Item = (State, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.existing.next()
    }
}

impl From<std::vec::Vec<(State, u64)>> for ExistingSuccessors {
    fn from(existing: std::vec::Vec<(State, u64)>) -> Self {
        Self {
            existing: existing.into_iter(),
        }
    }
}

// struct LazySuccessors<'a> {
//     state: &'a State,
//     cur_subdir: Option<(&'a PathBuf, &'a HashMap<PathBuf, Vec<Entry>>)>,
//     subdir_iter: std::collections::hash_map::Iter<'a, PathBuf, HashMap<PathBuf, Vec<Entry>>>,

//     cur_other_root_iter: std::collections::hash_map::Iter<'a, PathBuf, (u64, u64, u64)>,
//     cur_other_root: Option<(&'a PathBuf, &'a (u64, u64, u64))>,
// }

// impl<'a> LazySuccessors<'a> {
//     fn new(state: &'a State) -> Self {
//         let mut subdir_iter = state.data.iter();
//         let cur_subdir = subdir_iter.next();
//         let mut cur_other_root_iter = state.roots.iter();
//         let cur_other_root = cur_other_root_iter.next();
//         Self {
//             state,
//             cur_subdir,
//             subdir_iter,

//             cur_other_root_iter,
//             cur_other_root,
//         }
//     }

//     fn advance(&mut self) -> Option<()> {
//         let mut cur_other_root = self.cur_other_root_iter.next();
//         if self.cur_other_root == cur_other_root {
//             cur_other_root = self.cur_other_root_iter.next();
//         }
//         if cur_other_root.is_some() {
//             self.cur_other_root = cur_other_root;
//             return Some(());
//         }
//         None
//     }
// }

// impl<'a> Iterator for LazySuccessors<'a> {
//     type Item = (State, u64);

//     fn next(&mut self) -> Option<Self::Item> {
//         None
//         // if self.advance().is_none() {
//         //     return None;
//         // }
//         //         let mut e = self.state.data.clone();
//         //         // Remove from current subdir
//         //         let mut s = e.remove(subdir).unwrap();
//         //         let mut z = s.remove(other_root).unwrap();
//         //         z.push(entry.clone());
//         //         s.insert(other_root.clone(), z);
//         //         s.insert(root.clone(), new_entries.clone());
//         //         e.insert(subdir.clone(), s);

//         //         debug!(
//         //             "{:?}: move {:?} from {:?} to {:?}",
//         //             subdir, entry, root, other_root
//         //         );
//         //         debug!("{:#?} -> {:#?}", self.data, e);

//         //         let src = root.join(subdir).join(entry.subpath.clone());
//         //         let tgt = other_root.join(subdir).join(entry.subpath.clone());
//         //         let mut moves = self.moves.clone();
//         //         moves.push(Move { source: src, target: tgt });
//         //         Some((State {
//         //                 roots: self.state.roots.clone(),
//         //                 data: e,
//         //                 moves,
//         //                 total_entries: self.state.total_entries,
//         //             },
//         //             cost,
//         //         ))
//     }
// }

#[cfg(test)]
mod test {
    use std::{collections::HashMap, path::PathBuf};

    use crate::*;
}
