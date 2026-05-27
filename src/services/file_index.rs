use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use iced::widget::sensor::Key;
use serde::{Deserialize, Serialize};

use crate::services::local_storage::{LocalStorageService, ObjectType};

#[derive(Debug, Serialize, Default, Deserialize, Clone)]
#[serde(default)]
pub struct FileIndex {
    root_dir: PathBuf,
    root_dir_id: String,
    entries: HashMap<String, IndexEntry>, // drive_id -> entry
    #[serde(skip)]
    by_path: HashMap<PathBuf, String>, // rel_path to drive_id
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct IndexEntry {
    parent_id: String,
    name: String,
    is_dir: bool,
}

impl FileIndex {
    pub fn new(root_dir: PathBuf, root_dir_id: String) -> Self {
        Self {
            root_dir,
            root_dir_id,
            ..Default::default()
        }
    }
    pub fn load(root_folder_id: String) -> Self {
        let mut file_index = LocalStorageService::load_object::<FileIndex>(ObjectType::FileIndex)
            .unwrap_or_default();
        if file_index.root_dir_id != root_folder_id {
            file_index.root_dir_id = root_folder_id;
        }
        file_index.reload_cache_by_path();

        file_index
    }

    pub fn reload_cache_by_path(&mut self) {
        self.by_path.clear();

        let mut children_map: HashMap<&str, Vec<&str>> = HashMap::new();
        for (id, entry) in self.entries.iter() {
            children_map
                .entry(&entry.parent_id)
                .or_default()
                .push(id.as_str());
        }

        let mut stack = vec![(self.root_dir_id.as_str(), PathBuf::new())];

        while let Some((id, path)) = stack.pop() {
            self.by_path.insert(path.clone(), id.to_string());
            if let Some(children) = children_map.get(id) {
                for child in children {
                    if let Some(child_name) = self
                        .entries
                        .get(&child.to_string())
                        .map(|c| c.name.as_str())
                    {
                        stack.push((*child, path.join(child_name)));
                    }
                }
            }
        }
    }

    pub fn save(&self) {
        LocalStorageService::save_object(self, ObjectType::FileIndex);
    }

    pub fn get_object_name(&self, object_id: &String) -> Option<&String> {
        self.entries.get(object_id).map(|o| &o.name)
    }

    pub fn get_file_id(&self, path: PathBuf) -> Option<&String> {
        let relative_path = self.get_relative_path(path);
        if relative_path.eq(Path::new("")) {
            Some(&self.root_dir_id)
        } else {
            self.by_path.get(&relative_path)
        }
    }

    pub fn put_file_id(&mut self, path: PathBuf, file_id: String) {
        let is_dir = path.is_dir();
        let relative_path = self.get_relative_path(path);

        let entry = IndexEntry {
            parent_id: self.get_parent_id(&relative_path),
            name: relative_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            is_dir,
        };

        self.entries.insert(file_id.clone(), entry);
        self.by_path.insert(relative_path, file_id);
    }

    pub fn remove(&mut self, path: PathBuf) {
        if let Some(drive_id) = self.by_path.remove(&self.get_relative_path(path)) {
            let mut to_remove = vec![drive_id];
            let mut i: usize = 0;
            loop {
                if i >= to_remove.len() {
                    break;
                }

                if let Some(entry) = self.entries.remove(&to_remove[i])
                    && entry.is_dir
                {
                    let current_for_remove = to_remove[i].clone();
                    for (id, dir_entry) in self.entries.iter() {
                        if dir_entry.parent_id.eq(&current_for_remove) {
                            to_remove.push(id.to_string());
                        }
                    }
                }

                i += 1;
            }
        }
    }

    fn get_relative_path(&self, path: PathBuf) -> PathBuf {
        if path.is_absolute() {
            path.strip_prefix(&self.root_dir).unwrap().to_path_buf()
        } else {
            path
        }
    }

    fn get_parent_id(&self, path: &PathBuf) -> String {
        let root_relative_path = Path::new("");
        match path.parent() {
            Some(parent_path) if parent_path.eq(root_relative_path) => self.root_dir_id.clone(),
            Some(parent_path) => {
                // safe to unwrap here, because
                self.by_path
                    .get(&parent_path.to_path_buf())
                    .unwrap()
                    .clone()
            }
            None => {
                todo!()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{FileIndex, IndexEntry};
    use rstest::rstest;

    fn setup() -> FileIndex {
        let mut entries = HashMap::new();
        entries.insert(
            "id_1".to_string(),
            IndexEntry {
                parent_id: "id_0".to_string(),
                name: "a".to_string(),
                is_dir: true,
            },
        );
        entries.insert(
            "id_2".to_string(),
            IndexEntry {
                parent_id: "id_1".to_string(),
                name: "mario.txt".to_string(),
                is_dir: false,
            },
        );
        entries.insert(
            "id_3".to_string(),
            IndexEntry {
                parent_id: "id_1".to_string(),
                name: "b".to_string(),
                is_dir: true,
            },
        );
        entries.insert(
            "id_4".to_string(),
            IndexEntry {
                parent_id: "id_3".to_string(),
                name: "s.txt".to_string(),
                is_dir: false,
            },
        );
        entries.insert(
            "id_5".to_string(),
            IndexEntry {
                parent_id: "id_0".to_string(),
                name: "c.txt".to_string(),
                is_dir: false,
            },
        );

        let mut index = FileIndex {
            root_dir: PathBuf::from("/root/dir"),
            root_dir_id: "id_0".to_string(),
            entries,
            by_path: HashMap::new(),
        };
        index.reload_cache_by_path();
        index
    }

    #[rstest]
    #[case::remove_folder_via_absolute_path("/root/dir/a", vec!["a/mario.txt", "a/b", "a/b/s.txt", "a"])]
    #[case::remove_folder_via_relative_path("a", vec!["a/mario.txt", "a/b", "a/b/s.txt", "a"])]
    #[case::remove_file_via_absolute_path("/root/dir/c.txt", vec!["c.txt"])]
    fn test_remove(#[case] object_path: &str, #[case] deleted_objects: Vec<&str>) {
        let mut index = setup();
        let object_path = PathBuf::from(object_path);
        let object_id = index.get_file_id(object_path.clone()).unwrap();

        let missing = get_all_affected_ids(object_id, &index);

        assert_eq!(deleted_objects.len(), missing.len());

        index.remove(object_path);
        index.reload_cache_by_path();

        for path in deleted_objects {
            assert!(index.get_file_id(PathBuf::from(path)).is_none());
        }
    }

    #[rstest]
    #[case::ensure_rename_folder_cascade_renames("/root/dir/a", "/root/dir/b", vec!["b/mario.txt", "b/b", "b/b/s.txt", "b"])]
    #[case::ensure_rename_file("/root/dir/a/mario.txt", "/root/dir/a/mario1.txt", vec!["/root/dir/a/mario1.txt"])]
    #[case::ensure_rename_moves_a_file_to_different_folder("/root/dir/a/mario.txt", "/root/dir/mario.txt", vec!["/root/dir/mario.txt"])]
    #[case::ensure_rename_moves_a_file_to_different_folder("/root/dir/a/mario.txt", "/root/dir/a/b/mario.txt", vec!["/root/dir/a/b/mario.txt"])]
    #[case::ensure_rename_moves_a_folder_to_different_folder("/root/dir/a/b", "/root/dir/b", vec!["/root/dir/b", "/root/dir/b/s.txt"])]
    fn test_rename(#[case] from: &str, #[case] to: &str, #[case] renamed_objects: Vec<&str>) {
        let mut index = setup();

        let from_id = index.get_file_id(PathBuf::from(from)).unwrap();
        let affected_ids = get_all_affected_ids(from_id, &index);
        assert_eq!(renamed_objects.len(), affected_ids.len());

        index.put_file_id(PathBuf::from(to), from_id.to_string());
        index.reload_cache_by_path();

        for path in renamed_objects {
            assert!(
                index
                    .get_file_id(PathBuf::from(path))
                    .map(|id| { affected_ids.contains(id) })
                    .unwrap_or(false)
            );
        }
    }

    fn get_all_affected_ids(object_id: &String, file_index: &FileIndex) -> Vec<String> {
        let mut parents = vec![object_id];
        let mut affected = vec![object_id.clone()];

        while let Some(parent) = parents.pop() {
            for (id, entry) in file_index
                .entries
                .iter()
                .filter(|(_, e)| e.parent_id.eq(parent))
            {
                if entry.is_dir {
                    parents.push(id);
                }

                affected.push(id.clone());
            }
        }

        affected
    }
}
