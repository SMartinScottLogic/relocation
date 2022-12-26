use clap::Parser;
use log::{debug, error, info, trace};
use pathfinding::prelude::idastar;
use std::{collections::HashMap, ops::AddAssign, os::unix::fs::MetadataExt, path::PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Path(s) to search for files within.
    pub root: Vec<String>,
    /// Should plan be executed
    #[clap(long)]
    pub execute: bool
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct State {
    roots: Vec<PathBuf>,
    // subdir -> root: Vec<(subpath, size)>
    data: HashMap<PathBuf, HashMap<PathBuf, Vec<Entry>>>,
    moves: Vec<Move>,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Entry {
    size: u64,
    subpath: PathBuf,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Move {
    pub source: PathBuf,
    pub target: PathBuf,
}

impl State {
    pub fn moves(&self) -> Vec<Move> {
        self.moves.clone()
    }
}

impl State {
    pub fn relocate(&self) -> Option<(Vec<Move>, u64)> {
        let r = idastar(
            self,
            |s| s.successors(),
            |s| s.heuristic(),
            |s| s.success(),
        );
        if r.is_none() {
            error!("No complete relocation found. Possibly try each subdir in turn.");
            return None;
        }
        let (moves, cost) = r.unwrap();
        if moves.len() == 1 {
            info!("Already fully relocated");
            return None;
        }
        let moves = moves.last().unwrap().moves();
        info!(
            "Relocation solution found, with {} moves, costing {}: {:#?}",
            moves.len() - 1,
            cost,
            moves
        );
        Some((moves, cost))    
    }
}

impl State {
    fn successors(&self) -> Vec<(State, u64)> {
        let mut result = Vec::new();
        for (subdir, roots) in &self.data {
            let all_roots = roots.keys().cloned().collect::<Vec<_>>();
            for (root, entries) in roots {
                for entry in entries {
                    let cost = entry.size;
                    let new_entries = entries
                        .iter()
                        .filter(|e| *e != entry)
                        .cloned()
                        .collect::<Vec<_>>();
                    for other_root in &all_roots {
                        if root == other_root {
                            continue;
                        }
                        let mut e = self.data.clone();
                        // Remove current subdir
                        let mut s = e.remove(subdir).unwrap();
                        let mut z = s.remove(other_root).unwrap();
                        z.push(entry.clone());
                        s.insert(other_root.clone(), z);
                        s.insert(root.clone(), new_entries.clone());
                        e.insert(subdir.clone(), s);

                        debug!(
                            "{:?}: move {:?} from {:?} to {:?}",
                            subdir, entry, root, other_root
                        );
                        debug!("{:#?} -> {:#?}", self.data, e);

                        let src = root.join(subdir).join(entry.subpath.clone());
                        let tgt = other_root.join(subdir).join(entry.subpath.clone());
                        let mut moves = self.moves.clone();
                        moves.push(Move { source: src, target: tgt });
                        result.push((
                            State {
                                roots: self.roots.clone(),
                                data: e,
                                moves,
                            },
                            cost,
                        ));
                    }
                }
            }
        }
        result
    }

    fn heuristic(&self) -> u64 {
        self.data
            .iter()
            .map(|(subpath, roots)| State::heuristic_cost_full_drain(subpath, roots))
            .sum()
    }

    fn heuristic_cost_full_drain(subpath: &PathBuf, roots: &HashMap<PathBuf, Vec<Entry>>) -> u64 {
        // Calculate total size of all files within this subpath of all roots
        let total: u64 = roots
            .iter()
            .flat_map(|(_root, entries)| entries)
            .map(|entry| entry.size)
            .sum();
        // Calculate the minimum cost of moving all files to each root (total within that root, less the total)
        let min = roots
            .iter()
            .map(|(_root, entries)| {
                let root_total: u64 = entries.iter().map(|entry| entry.size).sum();
                total - root_total
            })
            .min()
            .unwrap();
        debug!("{subpath:?}: {total} {min}");
        min
    }

    fn success(&self) -> bool {
        !self.data.iter().any(|(_subpath, roots)| {
            roots
                .iter()
                .filter(|(_root, entries)| !entries.is_empty())
                .count()
                > 1
        })
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
    fn scan(&mut self, root: &str) {
        let cur_dir = std::env::current_dir().unwrap();
        let root = cur_dir.join(root).canonicalize().unwrap();
        if self.roots.contains(&root) {
            error!("Skipping duplicate scan root: {root:?}");
            return;
        }
        self.roots.push(root.clone());
        let root_dev_id = root.metadata().unwrap().dev();
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
            let root = root.clone();

            // subdir -> root: Vec<(subpath, size)>
            self.data
                .entry(subdir)
                .or_default()
                .entry(root)
                .or_default()
                .push(Entry { size, subpath });
        }
    }
}
