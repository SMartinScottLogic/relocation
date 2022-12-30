use std::{collections::HashMap, path::PathBuf};

use log::{debug, info};

use crate::{filesystem::FileSystem, Entry, State};

#[derive(Debug)]
pub struct LazySuccessors {
    roots: Vec<(PathBuf, FileSystem)>,
    entries: Vec<Entry>,
    usage: HashMap<PathBuf, HashMap<PathBuf, u64>>,
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

impl From<&State> for LazySuccessors {
    fn from(state: &State) -> Self {
        info!("LazySuccessors::from({state:?})");
        let roots = state
            .roots
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let entries = state.entries.clone();
        let usage = state.usage.clone();
        Self {
            //state: state.to_owned(),
            cur_entry_idx: 0,
            cur_root_idx: 0,
            roots,
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
    type Item = (State, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let mut cur_entry = self.entries.get(self.cur_entry_idx)?;
        let cur_root = loop {
            let cur_root = self.roots.get(self.cur_root_idx);
            self.cur_root_idx += 1;
            if let Some(cur_root) = cur_root {
                // TODO skip if insufficient space in 'cur_root'
                if cur_root.1.blocks_available < cur_root.1.blocks(cur_entry.size) {
                    debug!(
                        "Cannot move {:?} to {:?} ({} of {} blocks available)",
                        cur_entry,
                        cur_root,
                        cur_root.1.blocks_available,
                        cur_root.1.blocks(cur_entry.size)
                    );
                    continue;
                }
                if *cur_root.0 != cur_entry.root {
                    break cur_root;
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
        debug!("{:?} {:?} {:?}", cur_entry, cur_root, self.roots);
        info!(
            "move {:?} to {:?}",
            cur_entry
                .root
                .join(&cur_entry.subdir)
                .join(&cur_entry.subpath),
            cur_root.0.clone()
        );
        let state = Self::new_state(cur_entry, &self.entries, &self.roots, cur_root, &self.usage);
        Some((state, cur_entry.size))
    }
}

impl LazySuccessors {
    fn new_state(
        entry: &Entry,
        entries: &[Entry],
        roots: &[(PathBuf, FileSystem)],
        (other_root, other_filesystem): &(PathBuf, FileSystem),
        usage: &HashMap<PathBuf, HashMap<PathBuf, u64>>,
    ) -> State {
        let mut entries = entries
            .iter()
            .filter(|e| *e != entry)
            .cloned()
            .collect::<Vec<_>>();
        let effective_size = other_filesystem.effective_size(entry.size);
        info!(
            "Candidate: move {:?} to {:?} (cost {} for {:?})",
            entry.root.join(&entry.subdir).join(entry.subpath.clone()),
            other_root.join(&entry.subdir).join(entry.subpath.clone()),
            effective_size,
            other_filesystem
        );
        // Modify free blocks in roots
        let mut roots = roots.iter().cloned().collect::<HashMap<_, _>>();
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
        info!("- new roots: {roots:?}");
        // Modify usage
        info!("usage was: {usage:?}");
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
        info!("usage now: {usage:?}");
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
            roots: HashMap::new(),
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
        let entries = vec![Entry {
            size: 5,
            root: PathBuf::from("a"),
            subdir: PathBuf::from(""),
            subpath: PathBuf::from("test"),
        }];
        let usage = entries.iter().fold(
            HashMap::<PathBuf, HashMap<PathBuf, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir.to_owned())
                    .or_default()
                    .entry(v.root.to_owned())
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            entries,
            usage,
        };
        assert_eq!(1, LazySuccessors::from(&state).count());
    }

    #[test]
    fn two_file_two_root_successor() {
        let roots = ["a", "b"]
            .into_iter()
            .enumerate()
            .map(|(id, n)| (PathBuf::from(n), FileSystem::new(id as u64, 4096, 1, false)))
            .collect();
        let entries = vec![
            Entry {
                size: 5,
                root: PathBuf::from("a"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test2"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<PathBuf, HashMap<PathBuf, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir.to_owned())
                    .or_default()
                    .entry(v.root.to_owned())
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
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
        let entries = vec![
            Entry {
                size: 10,
                root: PathBuf::from("a"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("a"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test2"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test3"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test4"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<PathBuf, HashMap<PathBuf, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir.to_owned())
                    .or_default()
                    .entry(v.root.to_owned())
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
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
        let entries = vec![
            Entry {
                size: 10,
                root: PathBuf::from("a"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("a"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test2"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test3"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from(""),
                subpath: PathBuf::from("test4"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<PathBuf, HashMap<PathBuf, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir.to_owned())
                    .or_default()
                    .entry(v.root.to_owned())
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
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
        let entries = vec![
            Entry {
                size: 10,
                root: PathBuf::from("a"),
                subdir: PathBuf::from("A"),
                subpath: PathBuf::from("test"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("a"),
                subdir: PathBuf::from("B"),
                subpath: PathBuf::from("test2"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from("A"),
                subpath: PathBuf::from("test3"),
            },
            Entry {
                size: 10,
                root: PathBuf::from("b"),
                subdir: PathBuf::from("B"),
                subpath: PathBuf::from("test4"),
            },
        ];
        let usage = entries.iter().fold(
            HashMap::<PathBuf, HashMap<PathBuf, u64>>::new(),
            |mut acc, v| {
                *acc.entry(v.subdir.to_owned())
                    .or_default()
                    .entry(v.root.to_owned())
                    .or_default() += 1;
                acc
            },
        );
        let state = State {
            roots,
            entries,
            usage,
        };
        assert_eq!(4, LazySuccessors::from(&state).count());
    }
}
