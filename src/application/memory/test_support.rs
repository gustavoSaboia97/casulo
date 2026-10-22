//! What the memory tests share: gits that see one branch and no files,
//! or the files a test sets.
// The whole file is test code: it is compiled only for `cargo test`, and
// the architecture test does not check what it imports.
#![cfg(test)]

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;

use crate::application::memory::domain::RepoPath;
use crate::application::memory::domain::event::BranchHead;
use crate::application::memory::ports::Git;

/// A git on branch `main` that lists no files and hashes every file as
/// `disk`.
pub struct FakeGit;

impl Git for FakeGit {
    fn ls_files(&self, _directory: &Path) -> Result<Vec<(RepoPath, String)>> {
        Ok(Vec::new())
    }
    fn hash_file(&self, _directory: &Path, _path: &RepoPath) -> Result<String> {
        Ok("disk".to_string())
    }
    fn current_branch(&self, _directory: &Path) -> Result<Option<String>> {
        Ok(Some("main".to_string()))
    }
    fn local_branches(&self, _directory: &Path) -> Result<Vec<BranchHead>> {
        Ok(Vec::new())
    }
    fn ls_tree(&self, _directory: &Path, _commit: &str) -> Result<Vec<(RepoPath, String)>> {
        Ok(Vec::new())
    }
}

/// A git like `FakeGit` that lists the files a test sets. The lock lets a
/// test change them while a server shares the git.
pub struct ListingGit {
    files: Mutex<Vec<(RepoPath, String)>>,
}

impl ListingGit {
    /// A git that lists `files`, given as `(path, hash)`.
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: Mutex::new(listing(files)),
        }
    }

    /// Lists `files` from now on.
    pub fn set(&self, files: &[(&str, &str)]) {
        *self.files.lock().unwrap() = listing(files);
    }
}

fn listing(files: &[(&str, &str)]) -> Vec<(RepoPath, String)> {
    files
        .iter()
        .map(|(path, hash)| (RepoPath::parse(path).unwrap(), hash.to_string()))
        .collect()
}

impl Git for ListingGit {
    fn ls_files(&self, _directory: &Path) -> Result<Vec<(RepoPath, String)>> {
        Ok(self.files.lock().unwrap().clone())
    }
    fn hash_file(&self, directory: &Path, path: &RepoPath) -> Result<String> {
        FakeGit.hash_file(directory, path)
    }
    fn current_branch(&self, directory: &Path) -> Result<Option<String>> {
        FakeGit.current_branch(directory)
    }
    fn local_branches(&self, directory: &Path) -> Result<Vec<BranchHead>> {
        FakeGit.local_branches(directory)
    }
    fn ls_tree(&self, directory: &Path, commit: &str) -> Result<Vec<(RepoPath, String)>> {
        FakeGit.ls_tree(directory, commit)
    }
}
