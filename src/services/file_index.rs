use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::services::local_storage::{LocalStorageService, ObjectType};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileIndex {
    #[serde(flatten)]
    org_files: HashMap<PathBuf, String>,
}

impl FileIndex {
    pub fn load() -> Self {
        LocalStorageService::load_object::<FileIndex>(ObjectType::FileIndex).unwrap_or_default()
    }

    pub fn save(&self) {
        LocalStorageService::save_object(self, ObjectType::FileIndex);
    }

    pub fn get_file_id(&self, path: &PathBuf) -> Option<&String> {
        self.org_files.get(path)
    }

    pub fn put_file_id(&mut self, path: PathBuf, file_id: String) {
        self.org_files.insert(path, file_id);
    }
}
