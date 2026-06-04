pub mod auth;
pub mod drive;
pub mod file_index;
pub mod http;
pub mod local_storage;
pub mod org;
pub mod resolver;
pub mod revisions_cache;

mod notify;
#[cfg(target_os = "macos")]
pub(crate) use notify::macos::notify_folder_changed;
#[cfg(windows)]
pub(crate) use notify::windows::notify_folder_changed;
