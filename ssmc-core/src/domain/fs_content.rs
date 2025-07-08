use std::collections::BTreeSet;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSource {
    LocalPath(PathBuf),
    RemoteUrl(Url),
    InMemory(Vec<u8>),
    Directory(),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    rel_path: PathBuf,
    executable: bool,
    source: FileSource,
}

impl PartialOrd for FileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.rel_path.partial_cmp(&other.rel_path)
    }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rel_path.cmp(&other.rel_path)
    }
}

impl FileEntry {
    pub fn new(rel_path: PathBuf, executable: bool, source: FileSource) -> Self {
        if !rel_path.is_relative() {
            panic!(
                "Path must be relative to a base directory, not absolute: {}",
                rel_path.display()
            );
        }

        Self {
            rel_path,
            executable,
            source,
        }
    }

    pub fn rel_path(&self) -> &PathBuf {
        &self.rel_path
    }

    pub fn executable(&self) -> bool {
        self.executable
    }

    pub fn source(&self) -> &FileSource {
        &self.source
    }

    pub fn with_prefix(self, prefix: &PathBuf) -> Self {
        if !self.rel_path.is_relative() {
            panic!(
                "Cannot add prefix to an absolute path: {}",
                self.rel_path.display()
            );
        }
        let mut new_path = prefix.clone();
        new_path.push(&self.rel_path);
        Self {
            rel_path: new_path,
            executable: self.executable,
            source: self.source,
        }
    }
}

#[derive(Debug)]
pub struct FileBundle {
    entries: BTreeSet<FileEntry>,
}

impl IntoIterator for FileBundle {
    type Item = FileEntry;
    type IntoIter = std::collections::btree_set::IntoIter<FileEntry>;

    fn into_iter(self) -> std::collections::btree_set::IntoIter<Self::Item> {
        self.entries.into_iter()
    }
}

impl FromIterator<FileEntry> for FileBundle {
    fn from_iter<T: IntoIterator<Item = FileEntry>>(iter: T) -> Self {
        let entries: BTreeSet<FileEntry> = iter.into_iter().collect();
        Self { entries }
    }
}

impl FileBundle {
    pub fn new(entries: Vec<FileEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /** ソート済み */
    pub fn entries(&self) -> Vec<&FileEntry> {
        self.entries.iter().collect()
    }

    pub fn into_entries(self) -> Vec<FileEntry> {
        self.entries.into_iter().collect()
    }

    pub fn with_prefix(self, prefix: &PathBuf) -> Self {
        let new_entries: BTreeSet<FileEntry> = self
            .entries
            .into_iter()
            .map(|entry| entry.with_prefix(prefix))
            .collect();
        Self {
            entries: new_entries,
        }
    }

    pub fn add_entry(&mut self, entry: FileEntry) {
        self.entries.insert(entry);
    }

    pub fn add_entries(&mut self, entries: Vec<FileEntry>) {
        self.entries.extend(entries);
    }

    pub fn concat(self, other: FileBundle) -> Self {
        let mut new_entries = self.entries;
        new_entries.extend(other.entries);
        Self {
            entries: new_entries,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
