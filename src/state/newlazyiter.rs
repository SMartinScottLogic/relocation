use std::{collections::HashMap, path::PathBuf};

use log::debug;

use crate::{filesystem::FileSystem, NewState};

use super::NewEntry;

#[derive(Debug)]
pub struct LazySuccessors {
    roots: Vec<FileSystem>,
    //subdirs: Vec<PathBuf>,
    entries: Vec<NewEntry>,
    usage: HashMap<(usize, usize), u64>,
    cur_entry_idx: usize,
    cur_root_idx: usize,
    //state: State,
    // roots: HashMap<PathBuf, FileSystem>,
    // entries: Vec<Entry>,
    // usage: HashMap<PathBuf, HashMap<PathBuf, u64>>,

    // entry_iter: std::slice::Iter<'a, Entry>,
    // cur_entry: Option<&'a Entry>,
    // root_iter: std::collections::hash_map::Iter<'a, PathBuf, FileSystem>,
}

impl From<&NewState> for LazySuccessors {
    fn from(state: &NewState) -> Self {
        debug!("LazySuccessors::from({state:?})");
        let roots = state.roots.clone();
        //let subdirs = state.subdirs.clone();
        let entries = state.entries.clone();
        let usage = state.usage.clone();
        Self {
            //state: state.to_owned(),
            cur_entry_idx: 0,
            cur_root_idx: 0,
            roots,
            //subdirs,
            entries,
            usage,
        }
        // let roots = state.roots.clone();
        // let entries = state.entries.clone();
        // let usage = state.usage.clone();

        // let mut entry_iter = entries.iter();
        // let cur_entry = entry_iter.next();
        // let root_iter = roots.iter();
        // Self {
        //     roots,
        //     entries,
        //     usage,

        //     entry_iter,
        //     cur_entry,
        //     root_iter,
        // }
    }
}

impl Iterator for LazySuccessors {
    type Item = (NewState, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let mut cur_entry = self.entries.get(self.cur_entry_idx)?;
        let cur_root_idx = loop {
            let cur_root_idx = self.cur_root_idx;
            let cur_root = self.roots.get(self.cur_root_idx);
            self.cur_root_idx += 1;
            if let Some(fs) = cur_root {
                // Skip if insufficient space in 'cur_root'
                if fs.blocks_available < fs.blocks(cur_entry.size) {
                    debug!(
                        "Cannot move {:?} to {:?} ({} of {} blocks available)",
                        cur_entry,
                        cur_root,
                        fs.blocks_available,
                        fs.blocks(cur_entry.size)
                    );
                    continue;
                }
                if cur_root_idx != cur_entry.root_idx {
                    break cur_root_idx;
                }
            }
            if cur_root.is_none() {
                debug!("advance cur_entry");
                self.cur_entry_idx += 1;
                cur_entry = self.entries.get(self.cur_entry_idx)?;
                debug!("restart root iter");
                self.cur_root_idx = 0;
            }
        };
        debug!("{:?} {:?} {:?}", cur_entry, cur_root_idx, self.roots);
        debug!("move {:?} to {:?}", cur_entry, cur_root_idx);
        let state = Self::new_state(
            cur_entry,
            &self.entries,
            &self.roots,
            cur_root_idx,
            &self.usage,
        );
        Some((state, cur_entry.size))
    }
}

impl LazySuccessors {
    fn new_state(
        entry: &NewEntry,
        entries: &[NewEntry],
        roots: &[FileSystem],
        other_root_idx: usize,
        usage: &HashMap<(usize, usize), u64>,
    ) -> NewState {
        let mut entries = entries
            .iter()
            .filter(|e| *e != entry)
            .cloned()
            .collect::<Vec<_>>();
        // Modify free blocks in roots
        let mut roots = roots.to_vec();
        debug!("original roots: {roots:?}");
        {
            // freeing of blocks
            let fs = roots.get_mut(entry.root_idx).unwrap();
            let freed_blocks = fs.blocks(entry.size);
            debug!("freed {} blocks from {:?}", freed_blocks, fs);
            fs.blocks_available += freed_blocks;
        }
        {
            // consumption of blocks
            let fs = roots.get_mut(other_root_idx).unwrap();
            let freed_blocks = fs.blocks(entry.size);
            debug!("consumed {} blocks from {:?}", freed_blocks, fs);
            fs.blocks_available += freed_blocks;
        }
        debug!("- new roots: {roots:?}");
        // Modify usage
        debug!("usage was: {usage:?}");
        let mut usage = usage.to_owned();
        *usage.entry((entry.subdir_idx, entry.root_idx)).or_default() -= 1;
        *usage.entry((entry.subdir_idx, other_root_idx)).or_default() += 1;
        debug!("usage now: {usage:?}");
        // Resultant state
        let mut entry = entry.to_owned();
        entry.root_idx = other_root_idx;
        entries.push(entry);

        NewState {
            roots,
            entries,
            usage,
        }
    }
}

#[cfg(test)]
#[ctor::ctor]
fn init() {
    use crate::setup_logger;

    setup_logger(true);
}
mod test {
    use std::{collections::HashMap, path::PathBuf};

    use crate::{
        filesystem::FileSystem,
        state::{Entry, LazySuccessors},
        State,
    };

    #[test]
    fn empty_state_successors() {
        let state = State {
            roots: Vec::new(),
            subdirs: Vec::new(),
            entries: Vec::new(),
            usage: HashMap::new(),
        };
        assert_eq!(0, LazySuccessors::from(&state).count());
    }

    #[test]
    fn one_file_two_root_successor() {
        let roots = ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| (PathBuf::from(n), FileSystem::new(id as u64, 4096, 1, false)))
            .collect();
        let subdirs = vec![
            PathBuf::from("A"),
            PathBuf::from("B"),
            PathBuf::from("C"),
            PathBuf::from(""),
        ];
        let entries = vec![Entry {
            size: 5,
            root_idx: 0,
            subdir_idx: 3,
            subpath: PathBuf::from("test"),
        }];
        let usage = entries.iter().fold(
            HashMap::<usize, HashMap<usize, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir_idx)
                    .or_default()
                    .entry(v.root_idx)
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            subdirs,
            entries,
            usage,
        };
        let r = LazySuccessors::from(&state)
            .inspect(|s| log::info!("{s:?}"))
            .count();
        assert_eq!(1, r);
    }

    #[test]
    fn two_file_two_root_successor() {
        let roots = ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| (PathBuf::from(n), FileSystem::new(id as u64, 4096, 1, false)))
            .collect();
        let subdirs = vec![
            PathBuf::from("A"),
            PathBuf::from("B"),
            PathBuf::from("C"),
            PathBuf::from(""),
        ];
        let entries = vec![
            Entry {
                size: 5,
                root_idx: 0,
                subdir_idx: 3,
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 3,
                subpath: PathBuf::from("test2"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<usize, HashMap<usize, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir_idx)
                    .or_default()
                    .entry(v.root_idx)
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            subdirs,
            entries,
            usage,
        };
        assert_eq!(2, LazySuccessors::from(&state).count());
    }

    #[test]
    fn two_root_two_full() {
        let roots = ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| {
                (
                    PathBuf::from(n),
                    FileSystem::new(id as u64, 4096, if id == 2 { 1000 } else { 0 }, false),
                )
            })
            .collect();
        let subdirs = vec![
            PathBuf::from("A"),
            PathBuf::from("B"),
            PathBuf::from("C"),
            PathBuf::from(""),
        ];
        let entries = vec![
            Entry {
                size: 10,
                root_idx: 0,
                subdir_idx: 3,
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root_idx: 0,
                subdir_idx: 3,
                subpath: PathBuf::from("test2"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 3,
                subpath: PathBuf::from("test3"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 3,
                subpath: PathBuf::from("test4"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<usize, HashMap<usize, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir_idx)
                    .or_default()
                    .entry(v.root_idx)
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            subdirs,
            entries,
            usage,
        };
        assert_eq!(0, LazySuccessors::from(&state).count());
    }

    #[test]
    fn three_root_two_full_one_empty() {
        let roots = ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| {
                (
                    PathBuf::from(n),
                    FileSystem::new(id as u64, 4096, if id == 2 { 1000 } else { 0 }, false),
                )
            })
            .collect();
        let subdirs = vec![
            PathBuf::from("A"),
            PathBuf::from("B"),
            PathBuf::from("C"),
            PathBuf::from(""),
        ];
        let entries = vec![
            Entry {
                size: 10,
                root_idx: 0,
                subdir_idx: 3,
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root_idx: 0,
                subdir_idx: 3,
                subpath: PathBuf::from("test2"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 3,
                subpath: PathBuf::from("test3"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 3,
                subpath: PathBuf::from("test4"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<usize, HashMap<usize, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir_idx)
                    .or_default()
                    .entry(v.root_idx)
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            subdirs,
            entries,
            usage,
        };
        assert_eq!(4, LazySuccessors::from(&state).count());
    }

    #[test]
    fn three_root_two_full_one_empty_scratch() {
        let roots = ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| {
                (
                    PathBuf::from(n),
                    FileSystem::new(id as u64, 4096, if id == 2 { 1000 } else { 0 }, id == 2),
                )
            })
            .collect();
        let subdirs = vec![PathBuf::from("A"), PathBuf::from("B"), PathBuf::from("C")];
        let entries = vec![
            Entry {
                size: 10,
                root_idx: 0,
                subdir_idx: 0,
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root_idx: 0,
                subdir_idx: 1,
                subpath: PathBuf::from("test2"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 0,
                subpath: PathBuf::from("test3"),
            },
            Entry {
                size: 10,
                root_idx: 1,
                subdir_idx: 1,
                subpath: PathBuf::from("test4"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<usize, HashMap<usize, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir_idx)
                    .or_default()
                    .entry(v.root_idx)
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            subdirs,
            entries,
            usage,
        };
        assert_eq!(4, LazySuccessors::from(&state).count());
    }
}
