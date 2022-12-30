use std::path::Path;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FileSystem {
    id: u64,
    block_size: u64,
    pub(crate) blocks_available: u64,
    scratch: bool,
}

impl FileSystem {
    pub fn new(id: u64, block_size: u64, blocks_available: u64, scratch: bool) -> Self {
        Self {
            id,
            block_size,
            blocks_available,
            scratch,
        }
    }
    pub fn blocks(&self, size: u64) -> u64 {
        1 + (size / self.block_size)
    }

    pub fn effective_size(&self, size: u64) -> u64 {
        self.block_size.saturating_mul(self.blocks(size))
    }

    pub fn free_bytes(&self) -> u64 {
        self.block_size.saturating_mul(self.blocks_available)
    }
}

impl FileSystem {
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
}

impl From<(&Path, bool)> for FileSystem {
    fn from((root, is_scratchpad): (&Path, bool)) -> Self {
        let (fsid, bsize, bavail) = Self::stats(root).unwrap();
        Self::new(fsid, bsize, bavail, is_scratchpad)
    }
}
