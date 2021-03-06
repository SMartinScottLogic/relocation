use std::fs;
use std::os::unix::fs::MetadataExt;
use walkdir::WalkDir;

fn scan(root: &str) -> Result<(), std::io::Error> {
    let walker = WalkDir::new(root).into_iter();
    for entry in walker {
        let entry = entry?;
        let meta = fs::metadata(entry.path())?;
        let dev_id = meta.dev();
        println!(
            "{} {} {:o} {:?} {} {} (@ {})",
            dev_id,
            entry.path().display(),
            entry.metadata()?.mode(),
            entry.metadata()?.is_dir(),
            entry.metadata()?.is_file(),
            entry.metadata()?.size(),
            std::env::current_dir()?.as_path().join(entry.path()).canonicalize()?.display()
        );
        println!("{:?}", entry.path().strip_prefix(root).unwrap().components());
    }
    Ok(())
}

fn main() -> Result<(), std::io::Error> {
    scan(".")?;
    scan("/dev")?;
    Ok(())
}
