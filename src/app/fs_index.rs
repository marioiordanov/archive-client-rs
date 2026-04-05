use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct FsIndex {
    pub(crate) path_to_inode: HashMap<PathBuf, u64>,
    pub(crate) inode_to_path: HashMap<u64, PathBuf>,
    pub(crate) direct_children: HashMap<PathBuf, Vec<(PathBuf, u64)>>,
}

impl FsIndex {
    pub(crate) fn scan(dir_root: &PathBuf) -> Self {
        let mut direct_children: HashMap<PathBuf, Vec<(PathBuf, u64)>> = HashMap::new();
        let entries = FsIndex::walk_dir(dir_root, &mut direct_children);

        Self {
            inode_to_path: entries.iter().map(|(k, v)| (*v, k.clone())).collect(),
            path_to_inode: entries.into_iter().collect(),
            direct_children,
        }
    }

    fn walk_dir(
        dir_root: &PathBuf,
        direct_children: &mut HashMap<PathBuf, Vec<(PathBuf, u64)>>,
    ) -> Vec<(PathBuf, u64)> {
        let mut paths = vec![];
        let mut dir_children = vec![];
        for entry in std::fs::read_dir(dir_root).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_symlink() {
                continue;
            } else if path.is_relative() {
                continue;
            } else if path.is_file() {
                let inode = path.metadata().unwrap().ino();
                paths.push((path.clone(), inode));
                dir_children.push((path, inode));
            } else if path.is_dir() {
                let inode = path.metadata().unwrap().ino();
                paths.push((path.clone(), inode));
                dir_children.push((path.clone(), inode));
                paths.append(&mut FsIndex::walk_dir(&path, direct_children));
            }
        }

        direct_children.insert(dir_root.clone(), dir_children);

        paths
    }

    pub(crate) fn is_folder(&self, path: &Path) -> bool {
        self.direct_children.contains_key(path)
    }
}
