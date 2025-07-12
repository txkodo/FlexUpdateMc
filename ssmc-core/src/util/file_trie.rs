use std::{
    collections::{HashMap, hash_map},
    path,
};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path(Vec<String>);

impl Path {
    pub fn new() -> Self {
        Path(Vec::new())
    }

    pub fn from_str(path: &str) -> Self {
        Path(
            path.split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
        )
    }

    pub fn components(&self) -> &Vec<String> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn pop(&mut self) -> Option<String> {
        self.0.pop()
    }

    pub fn push(&mut self, component: impl Into<String>) {
        self.0.push(component.into());
    }

    pub fn join(&self, other: impl Into<Path>) -> Path {
        let mut new_components = self.0.clone();
        new_components.extend(other.into().0);
        Path(new_components)
    }
}

impl From<&str> for Path {
    fn from(path: &str) -> Self {
        Path::from_str(path)
    }
}

impl From<String> for Path {
    fn from(path: String) -> Self {
        Path::from_str(&path)
    }
}

impl From<&String> for Path {
    fn from(path: &String) -> Self {
        Path::from_str(path)
    }
}

impl From<Vec<String>> for Path {
    fn from(components: Vec<String>) -> Self {
        Path(components)
    }
}

impl From<&[String]> for Path {
    fn from(components: &[String]) -> Self {
        Path(components.to_vec())
    }
}

impl From<Vec<&str>> for Path {
    fn from(components: Vec<&str>) -> Self {
        Path(components.into_iter().map(|s| s.to_string()).collect())
    }
}

impl From<&[&str]> for Path {
    fn from(components: &[&str]) -> Self {
        Path(components.iter().map(|s| s.to_string()).collect())
    }
}

impl From<&Path> for Path {
    fn from(components: &Path) -> Self {
        components.clone()
    }
}

#[derive(Debug, Clone)]
pub enum File {
    Inline(Vec<u8>),
    Url(Url),
    Path(path::PathBuf),
}

#[derive(Debug, Clone)]
pub struct Dir(HashMap<String, Entry>);

impl Dir {
    pub fn new() -> Dir {
        Dir(HashMap::new())
    }

    pub fn get(&self, path: impl Into<Path>) -> Option<&Entry> {
        let vpath = path.into();
        let components = vpath.components();

        if components.is_empty() {
            return None; // Root directory access not supported this way
        }

        let first = &components[0];
        let entry = self.0.get(first)?;

        if components.len() == 1 {
            Some(entry)
        } else {
            match entry {
                Entry::Dir(dir) => {
                    let remaining_path = Path(components[1..].to_vec());
                    dir.get(remaining_path)
                }
                Entry::File(_) => None, // Cannot traverse into a file
            }
        }
    }
    pub fn get_file(&self, path: impl Into<Path>) -> Option<&File> {
        match self.get(path) {
            Some(Entry::File(file)) => Some(file),
            _ => None,
        }
    }
    pub fn get_dir(&self, path: impl Into<Path>) -> Option<&Dir> {
        match self.get(path) {
            Some(Entry::Dir(dir)) => Some(dir),
            _ => None,
        }
    }

    pub fn put(&mut self, path: impl Into<Path>, entry: Entry) -> Result<(), Error> {
        let vpath: Path = path.into();
        let components = vpath.components();

        if components.is_empty() {
            return Err(Error::PathConflict); // Cannot put at root
        }

        if components.len() == 1 {
            let key = &components[0];
            self.0.insert(key.clone(), entry);
            Ok(())
        } else {
            let first = &components[0];

            // Get or create intermediate directory
            let intermediate_dir = match self.0.get_mut(first) {
                Some(Entry::Dir(dir)) => dir,
                Some(Entry::File(_)) => {
                    return Err(Error::PathConflict);
                }
                None => {
                    // Create intermediate directory
                    self.0.insert(first.clone(), Entry::Dir(Dir::new()));
                    match self.0.get_mut(first).unwrap() {
                        Entry::Dir(dir) => dir,
                        _ => unreachable!(),
                    }
                }
            };

            let remaining_path = Path(components[1..].to_vec());
            intermediate_dir.put(remaining_path, entry)
        }
    }

    pub fn put_file(&mut self, path: impl Into<Path>, file: File) -> Result<(), Error> {
        self.put(path, Entry::File(file))
    }
    pub fn put_dir(&mut self, path: impl Into<Path>, dir: Dir) -> Result<(), Error> {
        self.put(path, Entry::Dir(dir))
    }

    pub fn delete(&mut self, path: impl Into<Path>) -> bool {
        let vpath = path.into();
        let components = vpath.components();

        if components.is_empty() {
            return false; // Cannot delete root
        }

        if components.len() == 1 {
            let key = &components[0];
            self.0.remove(key).is_some()
        } else {
            let first = &components[0];

            match self.0.get_mut(first) {
                Some(Entry::Dir(dir)) => {
                    let remaining_path = Path(components[1..].to_vec());
                    dir.delete(remaining_path)
                }
                _ => false, // Path doesn't exist or is a file
            }
        }
    }

    pub fn iter<'a>(&'a self) -> hash_map::Iter<'a, String, Entry> {
        self.0.iter()
    }

    pub fn iter_mut<'a>(&'a mut self) -> hash_map::IterMut<'a, String, Entry> {
        self.0.iter_mut()
    }

    pub fn into_iter(self) -> hash_map::IntoIter<String, Entry> {
        self.0.into_iter()
    }

    pub fn iter_all<'a>(&'a self) -> DirIterator<'a> {
        DirIterator::new(self, Path::new())
    }
}

pub struct DirIterator<'a> {
    stack: Vec<(Path, &'a Entry)>,
}

impl<'a> DirIterator<'a> {
    fn new(dir: &'a Dir, base_path: Path) -> Self {
        let mut stack = Vec::new();
        for (name, entry) in dir.0.iter() {
            let entry_path = base_path.join(name.clone());
            stack.push((entry_path, entry));
        }
        stack.reverse();
        Self { stack }
    }
}

impl<'a> Iterator for DirIterator<'a> {
    type Item = (Path, &'a Entry);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((path, entry)) = self.stack.pop() {
            if let Entry::Dir(dir) = entry {
                for (name, child_entry) in dir.0.iter() {
                    let child_path = path.join(name.clone());
                    self.stack.push((child_path, child_entry));
                }
            }
            Some((path, entry))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum Error {
    PathConflict,
}

#[derive(Debug, Clone)]
pub enum Entry {
    File(File),
    Dir(Dir),
}

impl Entry {
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File(_))
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vpath_creation() {
        let path = Path::new();
        assert!(path.is_empty());

        let path = Path::from_str("foo/bar/baz");
        assert_eq!(path.components(), &vec!["foo", "bar", "baz"]);

        let path = Path::from_str("/foo/bar/");
        assert_eq!(path.components(), &vec!["foo", "bar"]);

        let path = Path::from_str("");
        assert!(path.is_empty());
    }

    #[test]
    fn test_vpath_operations() {
        let mut path = Path::new();
        path.push("foo");
        path.push("bar");
        assert_eq!(path.components(), &vec!["foo", "bar"]);

        assert_eq!(path.pop(), Some("bar".to_string()));
        assert_eq!(path.components(), &vec!["foo"]);
    }

    #[test]
    fn test_vdir_basic_operations() {
        let mut root = Dir::new();
        let file = File::Inline(b"hello world".to_vec());

        // Test put_file
        assert!(root.put_file("test.txt", file).is_ok());

        // Test get_file
        let retrieved = root.get_file("test.txt");
        assert!(retrieved.is_some());
        match retrieved.unwrap() {
            File::Inline(data) => assert_eq!(data, b"hello world"),
            _ => panic!("Expected inline file"),
        }

        // Overwrite successfully
        let file2 = File::Inline(b"conflict".to_vec());
        assert!(root.put_file("test.txt", file2).is_ok());
    }

    #[test]
    fn test_vdir_nested_operations() {
        let mut root = Dir::new();
        let file = File::Inline(b"nested file".to_vec());

        // Test automatic intermediate directory creation
        assert!(root.put_file("dir1/dir2/nested.txt", file).is_ok());

        // Test retrieval
        let retrieved = root.get_file("dir1/dir2/nested.txt");
        assert!(retrieved.is_some());

        // Test intermediate directory exists
        let dir = root.get_dir("dir1");
        assert!(dir.is_some());

        let nested_dir = root.get_dir("dir1/dir2");
        assert!(nested_dir.is_some());
    }

    #[test]
    fn test_vdir_deletion() {
        let mut root = Dir::new();
        let file = File::Inline(b"to be deleted".to_vec());

        // Create file
        assert!(root.put_file("temp.txt", file).is_ok());
        assert!(root.get_file("temp.txt").is_some());

        // Delete file
        assert!(root.delete("temp.txt"));
        assert!(root.get_file("temp.txt").is_none());

        // Try to delete non-existent file
        assert!(!root.delete("nonexistent.txt"));
    }

    #[test]
    fn test_vdir_nested_deletion() {
        let mut root = Dir::new();
        let file = File::Inline(b"nested deletion".to_vec());

        // Create nested file
        assert!(root.put_file("a/b/c.txt", file).is_ok());
        assert!(root.get_file("a/b/c.txt").is_some());

        // Delete nested file
        assert!(root.delete("a/b/c.txt"));
        assert!(root.get_file("a/b/c.txt").is_none());

        // Directory structure should still exist
        assert!(root.get_dir("a/b").is_some());
    }

    #[test]
    fn test_path_conflict_scenarios() {
        let mut root = Dir::new();
        let file = File::Inline(b"file content".to_vec());

        // Create a file
        assert!(root.put_file("overwrite", file).is_ok());

        // Overwrite with a directory (should success)
        let dir = Dir::new();
        assert!(root.put_dir("overwrite", dir).is_ok());
    }

    #[test]
    fn test_path_conversion() {
        let mut root = Dir::new();
        let file = File::Inline(b"conversion test".to_vec());

        // Test string literal
        assert!(root.put_file("test1.txt", file).is_ok());

        // Test String
        let file2 = File::Inline(b"string test".to_vec());
        let path = "test2.txt".to_string();
        assert!(root.put_file(path, file2).is_ok());

        // Test Vec<&str>
        let file4 = File::Inline(b"vec test".to_vec());
        assert!(root.put_file(vec!["dir", "test4.txt"], file4).is_ok());

        // Test slice
        let file5 = File::Inline(b"slice test".to_vec());
        assert!(root.put_file(&["dir", "test5.txt"][..], file5).is_ok());

        // Test retrieval works with all types
        assert!(root.get_file("test1.txt").is_some());
        assert!(root.get_file("test2.txt".to_string()).is_some());
        assert!(root.get_file(vec!["dir", "test4.txt"]).is_some());
        assert!(root.get_file(&["dir", "test5.txt"][..]).is_some());
    }

    #[test]
    fn test_complex_directory_structure() {
        let mut root = Dir::new();

        // Create a complex directory structure
        let files = vec![
            ("src/main.rs", "fn main() {}"),
            ("src/lib.rs", "pub mod test;"),
            ("tests/integration.rs", "#[test] fn test() {}"),
            ("Cargo.toml", "[package]\nname = \"test\""),
            ("README.md", "# Test Project"),
        ];

        for (path, content) in files.iter() {
            let file = File::Inline(content.as_bytes().to_vec());
            assert!(root.put_file(*path, file).is_ok());
        }

        // Test all files can be retrieved
        for (path, content) in files.iter() {
            let retrieved = root.get_file(*path);
            assert!(retrieved.is_some());
            match retrieved.unwrap() {
                File::Inline(data) => assert_eq!(data, content.as_bytes()),
                _ => panic!("Expected inline file"),
            }
        }

        // Test directories exist
        assert!(root.get_dir("src").is_some());
        assert!(root.get_dir("tests").is_some());
    }
}
