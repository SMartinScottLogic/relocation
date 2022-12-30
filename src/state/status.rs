use std::{
    collections::{HashMap, HashSet},
    ops::AddAssign,
    os::unix::prelude::MetadataExt,
    path::PathBuf,
};

use log::{debug, error, info, trace};
use pathfinding::prelude::idastar;
use walkdir::WalkDir;

use crate::{
    filesystem::FileSystem,
    state::{ExistingSuccessors, LazySuccessors},
};

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct State {
    pub(crate) roots: HashMap<std::path::PathBuf, FileSystem>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) usage: HashMap<PathBuf, HashMap<PathBuf, u64>>,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Entry {
    pub(crate) size: u64,
    pub(crate) root: PathBuf,
    pub(crate) subdir: PathBuf,
    pub(crate) subpath: PathBuf,
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
        info!("{} files total", self.entries.len());
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
    pub fn new_state(
        entry: &Entry,
        entries: &[Entry],
        roots: &HashMap<PathBuf, FileSystem>,
        (other_root, other_filesystem): (&PathBuf, &FileSystem),
        usage: &HashMap<PathBuf, HashMap<PathBuf, u64>>,
    ) -> State {
        let mut entries = entries
            .iter()
            .filter(|e| *e != entry)
            .cloned()
            .collect::<Vec<_>>();
        let effective_size = other_filesystem.effective_size(entry.size);
        debug!(
            "Candidate: move {:?} to {:?} (cost {} for {:?})",
            entry.root.join(&entry.subdir).join(entry.subpath.clone()),
            other_root.join(&entry.subdir).join(entry.subpath.clone()),
            effective_size,
            other_filesystem
        );
        // Modify free blocks in roots
        let mut roots = roots.to_owned();
        debug!("original roots: {roots:?}");
        roots.entry(entry.root.clone()).and_modify(|fs| {
            // freeing of blocks
            let freed_blocks = fs.blocks(entry.size);
            debug!("freed {} blocks from {:?}", freed_blocks, entry.root);
            fs.blocks_available = freed_blocks;
        });
        roots.entry(other_root.to_path_buf()).and_modify(|fs| {
            // consumption of blocks
            let consumed_blocks = fs.blocks(entry.size);
            debug!("consumed {} blocks from {:?}", consumed_blocks, other_root);
            fs.blocks_available -= consumed_blocks;
        });
        debug!("new roots: {roots:?}");
        // Modify usage
        let mut usage = usage.to_owned();
        *usage
            .entry(entry.subdir.to_owned())
            .or_default()
            .entry(entry.root.clone())
            .or_default() -= 1;
        *usage
            .entry(entry.subdir.to_owned())
            .or_default()
            .entry(other_root.to_path_buf())
            .or_default() += 1;
        // Resultant state
        let mut entry = entry.to_owned();
        entry.root = other_root.to_owned();
        entries.push(entry);

        State {
            entries,
            roots,
            usage,
        }
    }

    fn successors(&self) -> Box<dyn Iterator<Item = (State, u64)>> {
        if self.entries.len() > 1 {
            let successors = LazySuccessors::from(self);
            info!("successors: {successors:?}");
            // info!(
            //     "num successors: {}",
            //     successors
            //         .inspect(|(state, cost)| {
            //             info!("cost: {cost}, state: {state:?}");
            //         })
            //         .count()
            // );
            // TODO Lazy resolution of successors
            //panic!("Too many entries");
            // let successors = LazySuccessors::new(self);
            Box::new(successors)
        } else {
            let mut num_tests = 0_u64;
            let mut result = Vec::new();
            for entry in &self.entries {
                for (other_root, other_fs) in &self.roots {
                    if entry.root == *other_root {
                        continue;
                    }
                    num_tests += 1;
                    if other_fs.free_bytes() < entry.size {
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

                    let cost = other_fs.effective_size(entry.size);
                    debug!(
                        "{}: Candidate: move {:?} to {:?} (cost {} for {:?})",
                        num_tests,
                        entry.root.join(&entry.subdir).join(entry.subpath.clone()),
                        other_root.join(&entry.subdir).join(entry.subpath.clone()),
                        cost,
                        other_fs
                    );
                    // Modify free blocks in roots
                    let mut roots = self.roots.clone();
                    roots.entry(entry.root.clone()).and_modify(|fs| {
                        // Pessimistic freeing of blocks
                        fs.blocks_available += fs.blocks(entry.size);
                    });
                    roots.entry(other_root.clone()).and_modify(|fs| {
                        // Pessimistic consumption of blocks
                        fs.blocks_available -= fs.blocks(entry.size);
                    });
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
        // TODO Adjust so scratchpad roots are empty
        !self
            .usage
            .iter()
            .any(|(_subpath, roots)| roots.values().filter(|v| **v != 0).count() > 1)
    }
}

impl AddAssign<&str> for State {
    fn add_assign(&mut self, rhs: &str) {
        self.scan(rhs, false);
    }
}

impl AddAssign<String> for State {
    fn add_assign(&mut self, rhs: String) {
        self.scan(&rhs, false);
    }
}

impl AddAssign<&String> for State {
    fn add_assign(&mut self, rhs: &String) {
        self.scan(rhs, false);
    }
}

impl State {
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

    fn scan(&mut self, root: &str, is_scratchpad: bool) {
        let cur_dir = match std::env::current_dir() {
            Ok(d) => d,
            Err(e) => {
                error!("Error scanning {:?} getting current directory: {}", root, e);
                return;
            }
        };
        let root = match cur_dir.join(root).canonicalize() {
            Ok(r) => r,
            Err(e) => {
                error!("Error scanning {:?} from {:?}: {}", root, cur_dir, e);
                return;
            }
        };
        if self.roots.contains_key(&root) {
            error!("Skipping duplicate scan root: {root:?}");
            return;
        }
        let root_dev_id = root.metadata().unwrap().dev();
        self.roots.insert(
            root.clone(),
            FileSystem::from((root.as_path(), is_scratchpad)),
        );
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
