pub const REDIRECT_URI: &str = "http://127.0.0.1:8001";
pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
pub const FILES_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
pub const ACTIVITY_URL: &str = "https://driveactivity.googleapis.com/v2/activity:query";
pub const LOCAL_FOLDER_BASE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/app-data");
