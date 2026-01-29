use std::{fs, io::Write, path::PathBuf};

use crate::app::state::UserProfile;

pub struct UserService;

impl UserService {
    // TODO: remove unwraps
    pub fn save_user_profile(profile: &UserProfile) {
        let path = Self::auth_cache_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            .unwrap();
        let mut file = fs::File::create(path).unwrap();
        file.write_all(json.as_bytes()).unwrap();
    }

    pub fn load_user_profile() -> Option<UserProfile> {
        let path = Self::auth_cache_path();
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn auth_cache_path() -> PathBuf {
        // store alongside repo in ./app-data/auth.json (already gitignored)

        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("app-data");
        path.push("auth.json");
        path
    }
}
