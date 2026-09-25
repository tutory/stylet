use crate::normalize;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

pub trait FileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn is_file(&self, path: &Path) -> bool;
}

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsFs;

impl FileSystem for OsFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// In-memory files, e.g. for tests and the playground.
#[derive(Debug, Clone, Default)]
pub struct MemoryFs {
    files: HashMap<PathBuf, Vec<u8>>,
}

impl MemoryFs {
    pub fn insert(&mut self, path: impl AsRef<Path>, contents: impl Into<Vec<u8>>) {
        self.files.insert(normalize(path.as_ref()), contents.into());
    }
}

impl FileSystem for MemoryFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(&normalize(path))
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(&normalize(path))
    }
}
