use std::{fs, io::Write, path::PathBuf};

use serde::{Serialize, de::DeserializeOwned};

use crate::constants::LOCAL_FOLDER_BASE;

pub struct LocalStorageService;

#[derive(Clone, Copy)]
pub enum ObjectType {
    UserProfile,
    Org,
    FileIndex,
}

impl LocalStorageService {
    // TODO: remove unwraps
    pub fn save_object<T: Serialize>(obj: &T, obj_type: ObjectType) {
        let path = Self::cache_path(obj_type);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let json = serde_json::to_string_pretty(obj)
            .map_err(std::io::Error::other)
            .unwrap();
        let mut file = fs::File::create(path).unwrap();
        file.write_all(json.as_bytes()).unwrap();
    }

    pub fn update_object<T, F>(obj_type: ObjectType, updater: F) -> Option<()>
    where
        T: DeserializeOwned + Serialize,
        F: FnOnce(&mut T),
    {
        let mut obj: T = Self::load_object(obj_type)?;
        updater(&mut obj);
        Self::save_object(&obj, obj_type);
        Some(())
    }

    pub fn load_object<T: DeserializeOwned>(obj_type: ObjectType) -> Option<T> {
        let path = Self::cache_path(obj_type);
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn cache_path(obj_type: ObjectType) -> PathBuf {
        let mut path = std::path::PathBuf::from(LOCAL_FOLDER_BASE);
        let filename = match obj_type {
            ObjectType::UserProfile => "auth.json",
            ObjectType::Org => "org.json",
            ObjectType::FileIndex => "file-index.json",
        };
        path.push(filename);
        path
    }
}
