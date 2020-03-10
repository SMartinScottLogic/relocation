use std::fs;
use std::os::unix::fs::MetadataExt;
use walkdir::WalkDir;

fn main() -> Result<(), std::io::Error> {
    let walker = WalkDir::new(".").into_iter();
    for entry in walker {
        let entry = entry?;
        let meta = fs::metadata(entry.path())?;
        let dev_id = meta.dev();
        println!(
            "{} {} {:o} {:?} {} {}",
            dev_id,
            entry.path().display(),
            entry.metadata()?.mode(),
            entry.metadata()?.is_dir(),
            entry.metadata()?.is_file(),
            entry.metadata()?.size()
        );
    }
    Ok(())
}
