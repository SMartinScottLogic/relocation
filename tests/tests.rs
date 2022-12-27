use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use relocation::Move;
use walkdir::WalkDir;

fn setup(test_dir: &str, files: &[(&str, &str)]) -> io::Result<()> {
    let test_dir = PathBuf::from(test_dir);

    if test_dir.exists() {
        fs::remove_dir_all(&test_dir)?;
    };
    for (file, contents) in files {
        fs::create_dir_all(test_dir.join(file).parent().unwrap())?;
        fs::write(test_dir.join(file), contents)?;
    }

    Ok(())
}

fn dump(test_dir: &str) {
    for e in WalkDir::new(test_dir) {
        let e = e.unwrap();
        if e.file_type().is_file() {
            println!("{:?} {:?} bytes", e.path(), e.metadata().map(|m| m.len()));
        }
    }
}

fn cleanup(test_dir: &str) -> io::Result<()> {
    fs::remove_dir_all(test_dir)?;

    Ok(())
}

#[test]
fn it_adds_two() {
    assert_eq!(4, 2 + 2);
}

#[test]
fn one_dir() -> io::Result<()> {
    let test_dir = "test_dir_one_dir";

    setup(
        test_dir,
        &[
            ("b/c/3.txt", "3"),
            ("b/c/2.txt", "hello"),
            ("b/c/4.txt", "1234567890"),
        ],
    )?;

    let mut state = relocation::State::default();
    state += test_dir.to_string() + "/b";

    assert_eq!(None, state.relocate());

    cleanup(test_dir)?;
    Ok(())
}

#[test]
fn two_dirs() -> io::Result<()> {
    let test_dir = "test_dir_two_dirs";

    setup(
        test_dir,
        &[
            ("b/c/3.txt", "3"),
            ("b/c/2.txt", "hello"),
            ("b/c/4.txt", "1234567890"),
            ("a/c/1.txt", "hello_world"),
            ("a/c/5.txt", "cat"),
        ],
    )?;

    dump(test_dir);

    let mut state = relocation::State::default();
    state += test_dir.to_string() + "/a";
    state += test_dir.to_string() + "/b";

    let r = state.relocate();

    assert!(r.is_some());
    let (moves, cost) = r.unwrap();
    println!("{cost}: {moves:?}");
    assert_eq!(8192, cost);
    assert_eq!(2, moves.len());

    let full_test_dir = PathBuf::from(test_dir).canonicalize().unwrap();
    assert!(moves.contains(&Move {
        source: full_test_dir.join("a/c/1.txt"),
        target: full_test_dir.join("b/c/1.txt")
    }));
    assert!(moves.contains(&Move {
        source: full_test_dir.join("a/c/5.txt"),
        target: full_test_dir.join("b/c/5.txt")
    }));

    cleanup(test_dir)?;
    Ok(())
}
