use std::{collections::HashMap, path::{Path, PathBuf}};

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
    by_path: HashMap<PathBuf, String> // rel_path to drive_id
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct IndexEntry {
    parent_id: String,
    name: String,
    is_dir: bool
}

impl FileIndex {
    pub fn new(root_dir: PathBuf, root_dir_id: String) -> Self {
        Self { root_dir, root_dir_id, ..Default::default() }
    }
    pub fn load(root_folder_id: String) -> Self {
        let mut file_index = LocalStorageService::load_object::<FileIndex>(ObjectType::FileIndex).unwrap_or_default();
        if file_index.root_dir_id != root_folder_id {
            file_index.root_dir_id = root_folder_id;
        }
        file_index.reload_cache_by_path();

        file_index
    }

    pub fn reload_cache_by_path(&mut self) {
        self.by_path.clear();
        let mut children_map:HashMap<&str, Vec<&str>> = HashMap::new();
        for (id, entry) in self.entries.iter() {
            children_map.entry(&entry.parent_id).or_default().push(id.as_str());
        }

        let mut stack = vec![(self.root_dir_id.as_str(), PathBuf::new())];

        while let Some((id, path)) = stack.pop() {
            self.by_path.insert(path.clone(), id.to_string());
            if let Some(children) = children_map.get(id) {
                for child in children {
                    if let Some(child_name) = self.entries.get(&child.to_string()).map(|c| c.name.as_str()){
                        stack.push((*child, path.join(child_name)));
                    }
                }
            }
        }
    }

    pub fn save(&self) {
        LocalStorageService::save_object(self, ObjectType::FileIndex);
    }

    pub fn get_file_id(&self, path: PathBuf) -> Option<&String> {
        let relative_path = self.get_relative_path(path);
        if relative_path.eq(Path::new("")) {
            Some(&self.root_dir_id)
        }else {
            self.by_path.get(&relative_path)
        }
    }

    pub fn put_file_id(&mut self, path: PathBuf, file_id: String) {
        let is_dir = path.is_dir();
        let relative_path = self.get_relative_path(path);
        let entry = IndexEntry {
            parent_id: self.get_parent_id(&relative_path),
            name: relative_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            is_dir,
        };

        self.entries.insert(file_id.clone(), entry);
        self.by_path.insert(relative_path, file_id);
    }

    pub fn remove_path(&mut self, path: &PathBuf) {
        if let Some(drive_id) = self.by_path.remove(path) {
            self.entries.remove(&drive_id);
        }
    }
    fn get_relative_path(&self, path: PathBuf) -> PathBuf {
        if path.is_absolute() {
            path.strip_prefix(&self.root_dir).unwrap().to_path_buf()
        }else {
            path
        }
    }

    fn get_parent_id(&self, path: &PathBuf) -> String {
        let root_relative_path = Path::new("");
        match path.parent() {
            Some(parent_path) if parent_path.eq(root_relative_path)=> self.root_dir_id.clone(),
            Some(parent_path) => {
                // safe to unwrap here, because
                self.by_path.get(&parent_path.to_path_buf()).unwrap().clone()
            }
            None => {
                todo!()
            },
        }
    }
}
