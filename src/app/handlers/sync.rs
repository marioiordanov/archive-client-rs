use std::path::{Path, PathBuf};

use iced::Task;

use crate::{
    ArchiveClient,
    app::message::{Message, SyncMessage},
    app::state::Screen,
    screens,
    services::drive::DriveService,
};

impl ArchiveClient {
    pub fn handle_sync_messages(&mut self, message: SyncMessage) -> Task<Message> {
        match (&mut self.screen, message) {
            (Screen::OrgSync(screen), SyncMessage::FsChanged(path)) => {
                Self::on_fs_changed(&mut self.app, screen, path)
            }
            (Screen::OrgSync(screen), SyncMessage::UploadFinished { path, result }) => {
                match result {
                    Ok(()) => screen.push_log(format!("Uploaded: {path}")),
                    Err(e) => screen.push_log(format!("Upload failed: {path} ({e})")),
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn on_fs_changed(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_sync::OrgSyncScreen,
        changed_path: PathBuf,
    ) -> Task<Message> {
        let Some(mapped_root_str) = state.org.config.local_folder_path.clone() else {
            return Task::none();
        };
        let mapped_root = PathBuf::from(mapped_root_str);

        // Filter out directories and common noise.
        if changed_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.') || n == ".DS_Store")
        {
            return Task::none();
        }

        if !is_path_under_root(&changed_path, &mapped_root) {
            return Task::none();
        }

        // If it is a directory event, ignore.
        if changed_path.is_dir() {
            return Task::none();
        }

        let relative = match changed_path.strip_prefix(&mapped_root) {
            Ok(r) => r,
            Err(_) => return Task::none(),
        };

        let relative_parent_segments: Vec<String> = relative
            .parent()
            .into_iter()
            .flat_map(|p| p.components())
            .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
            .collect();

        let drive_root_folder_id = state.org.config.archive_folder_id.clone();
        let access_token = state.session.user.access_token.clone();

        let changed_path_for_async = changed_path.clone();
        let changed_path_str_for_msg = changed_path.display().to_string();
        screen.push_log(format!("Changed: {changed_path_str_for_msg}"));

        Task::perform(
            async move {
                let parent_id = DriveService::ensure_remote_folder_path(
                    &drive_root_folder_id,
                    &relative_parent_segments,
                    &access_token,
                )
                .await?;

                DriveService::upload_local_file(
                    Path::new(&changed_path_for_async),
                    &parent_id,
                    &access_token,
                )
                    .await
            },
            move |result| {
                Message::Sync(SyncMessage::UploadFinished {
                    path: changed_path_str_for_msg,
                    result,
                })
            },
        )
    }
}

fn is_path_under_root(path: &Path, root: &Path) -> bool {
    // Use canonicalized best-effort check to avoid false positives with relative paths.
    let Ok(path_abs) = path.canonicalize() else {
        return false;
    };
    let Ok(root_abs) = root.canonicalize() else {
        return false;
    };

    path_abs.starts_with(root_abs)
}
