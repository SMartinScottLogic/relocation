use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    os::unix::prelude::MetadataExt,
    path::PathBuf,
};

use deepsize::DeepSizeOf;
use log::{debug, error, info, trace};
use pathfinding::prelude::{astar, idastar};
use walkdir::WalkDir;

use crate::{
    filesystem::FileSystem,
    state::{NewExistingSuccessors, NewLazySuccessors},
};

#[derive(Debug, Default)]
pub struct StateNames {
    roots: Vec<PathBuf>,
    subdirs: Vec<PathBuf>,
    subpath: Vec<PathBuf>,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, DeepSizeOf)]
pub struct State {
    pub(crate) roots: Vec<FileSystem>,
    pub(crate) entries: Vec<NewEntry>,
    // subdir_idx -> root_idx -> size
    pub(crate) usage: HashMap<(usize, usize), u64>,
}

impl Hash for State {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.entries.hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, DeepSizeOf)]
pub struct NewEntry {
    pub(crate) size: u64,
    pub(crate) root_idx: usize,
    pub(crate) subdir_idx: usize,
    pub(crate) subpath_idx: usize,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Move {
    source: NewEntry,
    target: NewEntry,
}

impl StateNames {
    fn name(&self, entry: &NewEntry) -> PathBuf {
        let root = self.roots.get(entry.root_idx).unwrap();
        let subdir = self.subdirs.get(entry.subdir_idx).unwrap();
        let subpath = self.subpath.get(entry.subpath_idx).unwrap();

        root.join(subdir).join(subpath)
    }
}

impl State {
    pub fn relocate(&self) -> Option<(Vec<Move>, u64)> {
        info!("{} files total", self.entries.len());
        info!("size of self: {}", self.deep_size_of());

        // Calculate moves
        let r = astar(self, |s| s.successors(), |s| s.heuristic(), |s| s.success());
        //let r = idastar(self, |s| s.successors(), |s| s.heuristic(), |s| s.success());
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
                    .map(|e| e.to_owned())
                    .collect::<HashSet<_>>();
                let tgt = b
                    .entries
                    .iter()
                    .map(|e| e.to_owned())
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
    fn new_state(
        entry_idx: usize,
        entry: &NewEntry,
        entries: &[NewEntry],
        roots: &[FileSystem],
        other_root_idx: usize,
        usage: &HashMap<(usize, usize), u64>,
    ) -> State {
        debug!("Move entry {}:{:?} to {}", entry_idx, entry, other_root_idx);
        let mut entries = entries.to_owned();
        let effective_size = roots[other_root_idx].effective_size(entry.size);
        // Modify free blocks in roots
        let mut roots = roots.to_owned();
        debug!("original roots: {roots:?}");
        {
            // freeing of blocks
            let root = roots.get_mut(entry.root_idx).unwrap();
            let freed_blocks = root.blocks(entry.size);
            debug!("freed {} blocks from {:?}", freed_blocks, root);
            root.blocks_available += freed_blocks;
        }
        {
            // consumption of blocks
            let root = roots.get_mut(other_root_idx).unwrap();
            let freed_blocks = root.blocks(entry.size);
            debug!("consumed {} blocks from {:?}", freed_blocks, root);
            root.blocks_available += freed_blocks;
        }
        debug!("new roots: {roots:?}");
        // Modify usage
        let mut usage = usage.to_owned();
        debug!(
            "original usage: {usage:?}; transfer from ({},{}) to ({},{})",
            entry.subdir_idx, entry.root_idx, entry.subdir_idx, other_root_idx
        );
        *usage.entry((entry.subdir_idx, entry.root_idx)).or_default() -= 1;
        *usage.entry((entry.subdir_idx, other_root_idx)).or_default() += 1;
        //usage[entry.subpath_idx][entry.root_idx] -= 1;
        //usage[entry.subpath_idx][other_root_idx] += 1;
        debug!("new usage: {usage:?}");
        // Resultant state
        entries[entry_idx].root_idx = other_root_idx;

        State {
            roots,
            entries,
            usage,
        }
    }

    fn successors(&self) -> Box<dyn Iterator<Item = (State, u64)>> {
        {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.hash(&mut hasher);
            info!(
                "Compute successors of {}: {}",
                std::hash::Hasher::finish(&hasher),
                self.heuristic()
            );
        }
        if self.entries.len() > 100 {
            Box::new(NewLazySuccessors::from(self))
            //panic!()
        } else {
            let mut num_tests = 0_u64;
            let mut result = Vec::new();
            for (entry_idx, entry) in self.entries.iter().enumerate() {
                for (other_root_idx, other_fs) in self.roots.iter().enumerate() {
                    if entry.root_idx == other_root_idx {
                        continue;
                    }
                    num_tests += 1;
                    if other_fs.free_bytes() < entry.size {
                        // No space to move into 'other_root' at this point
                        debug!("{}: No space for {:?} in {:?}", num_tests, entry, other_fs);
                        continue;
                    }
                    let cost = entry.size;
                    let new_state = Self::new_state(
                        entry_idx,
                        entry,
                        &self.entries,
                        &self.roots,
                        other_root_idx,
                        &self.usage,
                    );
                    result.push((new_state, cost));
                }
            }
            debug!("successors to {self:?}:");
            for r in &result {
                debug!("    {r:#?}");
            }
            Box::new(NewExistingSuccessors::from(result))
        }
    }

    fn heuristic(&self) -> u64 {
        let mut total = 0;
        for subdir_idx in 0..self.usage.len() {
            // Total size of all files within this subpath (over all roots)
            let mut subpath_total = 0;
            let mut root_total = Vec::new();
            root_total.resize(self.roots.len(), 0);
            for entry in &self.entries {
                if entry.subdir_idx != subdir_idx {
                    continue;
                }
                subpath_total += entry.size;
                root_total[entry.root_idx] += entry.size;
            }
            // Minimum cost of moving all files to each root (total within that root, less the overall total), skipping scratch roots
            let min_cost = root_total
                .iter()
                .enumerate()
                .filter(|(k, _)| !self.roots.get(*k).unwrap().scratch())
                .map(|(_, v)| subpath_total - *v)
                .min()
                .unwrap();
            total += min_cost;
        }
        total
    }

    fn success(&self) -> bool {
        debug!("{:?} {:#?}", self.roots, self.usage);
        let mut populated: HashSet<usize> = HashSet::new();
        for ((subdir_idx, root_idx), count) in &self.usage {
            // Scratchpad roots MUST be empty
            if *count > 0 {
                if self.roots[*root_idx].scratch() {
                    debug!("populated scratch: {root_idx}");
                    return false;
                }
                if !populated.insert(*subdir_idx) {
                    debug!("populated subdir: {subdir_idx}");
                    return false;
                }
            }
        }
        true
        // !self
        //     .usage
        //     .iter()
        //     .any(|((subdir_idx, root_idx), count)| {
        //         // Scratchpad roots MUST be empty
        //         roots.iter().enumerate().filter(|(k, v)| self.roots.get(*k).unwrap().scratch() && **v > 0).inspect(|v| debug!("scratch: {v:?}")).count() > 0 ||
        //         // Only one root per subdir holds files
        //         roots.iter().filter(|v| **v != 0).inspect(|v| debug!("populated root: {v:?}")).count() > 1
        //     })
    }
}

impl State {
    fn add_entry(&mut self, root_idx: usize, subdir_idx: usize, subpath_idx: usize, size: u64) {
        *self.usage.entry((subdir_idx, root_idx)).or_default() += 1;
        let entry = NewEntry {
            root_idx,
            subdir_idx,
            subpath_idx,
            size,
        };
        self.entries.push(entry);
    }

    pub fn scan(&mut self, names: &mut StateNames, root: &str, is_scratchpad: bool) {
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
        if names
            .roots
            .iter()
            .any(|existing_root| *existing_root == root)
        {
            error!("Skipping duplicate scan root: {root:?}");
            return;
        }
        let root_dev_id = root.metadata().unwrap().dev();
        names.roots.push(root.clone());
        let root_idx = names
            .roots
            .iter()
            .position(|existing_root| *existing_root == root)
            .unwrap();

        self.roots
            .push(FileSystem::from((root.as_path(), is_scratchpad)));
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
            let subdir_idx = match names.subdirs.iter().position(|v| *v == subdir) {
                Some(idx) => idx,
                None => {
                    names.subdirs.push(subdir.clone());
                    names.subdirs.iter().position(|v| *v == subdir).unwrap()
                }
            };
            let subpath = entry.path().strip_prefix(root.join(&subdir)).unwrap();
            let subpath = subpath.to_path_buf();
            let subpath_idx = match names.subpath.iter().position(|v| *v == subpath) {
                Some(idx) => idx,
                None => {
                    names.subpath.push(subpath.clone());
                    names.subpath.iter().position(|v| *v == subpath).unwrap()
                }
            };
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
            self.add_entry(root_idx, subdir_idx, subpath_idx, size);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, path::PathBuf};

    use crate::{
        filesystem::FileSystem,
        state::newstatus::{NewEntry, State},
        Entry,
    };

    fn usage(
        entries: &[NewEntry],
        roots: &[FileSystem],
        subdirs: &[PathBuf],
    ) -> HashMap<(usize, usize), u64> {
        entries.iter().fold(HashMap::new(), |mut acc, v| {
            *acc.entry((v.subdir_idx, v.root_idx)).or_default() += 1;
            acc
        })
    }

    #[test]
    fn success_spread() {
        let roots = ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| FileSystem::new(id as u64, 4096, 0, false))
            .collect::<Vec<_>>();
        let subdirs = vec![PathBuf::from("A"), PathBuf::from("B"), PathBuf::from("C")];
        let entries = vec![
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 0,
            },
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 1,
                subpath_idx: 1,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 0,
                subpath_idx: 2,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 1,
                subpath_idx: 3,
            },
        ];
        let usage = usage(&entries, &roots, &subdirs);
        let state = State {
            roots,
            entries,
            usage,
        };
        assert!(!state.success());
    }

    #[test]
    fn success_done() {
        let roots = ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| FileSystem::new(id as u64, 4096, 0, false))
            .collect::<Vec<_>>();
        let subdirs = vec![PathBuf::from("A"), PathBuf::from("B"), PathBuf::from("C")];
        let entries = vec![
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 0,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 1,
                subpath_idx: 1,
            },
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 2,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 1,
                subpath_idx: 3,
            },
        ];
        let usage = usage(&entries, &roots, &subdirs);
        let state = State {
            roots,
            entries,
            usage,
        };
        assert!(state.success());
    }

    #[test]
    fn success_done_scratch() {
        let roots = ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| {
                FileSystem::new(id as u64, 4096, if n == "c" { 1000 } else { 0 }, n == "c")
            })
            .collect::<Vec<_>>();
        let subdirs = vec![PathBuf::from("A"), PathBuf::from("B"), PathBuf::from("C")];
        let entries = vec![
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 0,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 2,
                subpath_idx: 1,
            },
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 2,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 1,
                subpath_idx: 3,
            },
        ];
        let usage = usage(&entries, &roots, &subdirs);
        let state = State {
            roots,
            entries,
            usage,
        };
        assert!(state.success());
    }

    #[test]
    fn success_scratch_used() {
        let roots = ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| {
                FileSystem::new(id as u64, 4096, if n == "c" { 1000 } else { 0 }, n == "c")
            })
            .collect::<Vec<_>>();
        let subdirs = vec![PathBuf::from("A"), PathBuf::from("B"), PathBuf::from("C")];
        let entries = vec![
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 0,
            },
            NewEntry {
                size: 10,
                root_idx: 2,
                subdir_idx: 2,
                subpath_idx: 1,
            },
            NewEntry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath_idx: 2,
            },
            NewEntry {
                size: 10,
                root_idx: 1,
                subdir_idx: 1,
                subpath_idx: 3,
            },
        ];
        let usage = usage(&entries, &roots, &subdirs);
        let state = State {
            roots,
            entries,
            usage,
        };
        assert!(!state.success());
    }
}
