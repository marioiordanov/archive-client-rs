use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
};

use iced::{Task, futures::stream};

use crate::{
    ArchiveClient, UserState,
    app::{
        message::{Message, SyncAction, SyncError, SyncMessage},
        state::{OrgConfig, OrgState, Screen, UserProfile},
    },
    screens,
    services::{
        drive::DriveService,
        file_index::FileIndex,
        local_storage::{LocalStorageService, ObjectType},
        resolver::Resolver,
    },
};

impl ArchiveClient {
    pub fn handle_sync_messages(&mut self, message: SyncMessage) -> Task<Message> {
        match (&self.app.user_state, &mut self.screen, message) {
            (
                UserState::OrgJoined { .. },
                Screen::OrgSync(_),
                SyncMessage::InitialSyncCompleted {root_dir, result: Ok(file_index)},
            ) => {
                LocalStorageService::update_object::<crate::app::state::OrgState, _>(
                    ObjectType::Org,
                    |org| org.status = crate::app::state::OrgStatus::Ready,
                );

                file_index.save();
                self.app
                    .user_state
                    .org_synced(Resolver::new(root_dir.clone(), file_index), root_dir);
                LocalStorageService::update_object::<OrgState, _>(ObjectType::Org, |org| {
                    org.status = crate::app::state::OrgStatus::Ready;
                });

                Task::none()
            }
            (
                UserState::OrgSynced { root_dir, root_folder_id, user_data, .. },
                Screen::OrgSync(screen),
                SyncMessage::ActionsReady(actions),
            ) => {
                let root_dir = root_dir.clone();
                let org_id = root_folder_id.clone();
                let access_token = user_data.access_token.clone();
                Self::on_sync_actions(&mut self.app, screen, actions, root_dir, org_id, access_token)
            }
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::UploadFinished { path, result },
            ) => match result {
                Ok(file) => {
                    screen.push_log(format!("Uploaded: {}", path.display()));
                    self.app.index.put_file_id(path, file.id);
                    Task::none()
                }
                Err(e) => {
                    screen.push_log(format!("Upload failed: {} ({e})", path.display()));
                    self.handle_error(e.into())
                }
            },
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::FolderEnsureFinished { path, result },
            ) => {
                match result {
                    Ok(_) => screen.push_log(format!("Ensured folder: {path}")),
                    Err(e) => screen.push_log(format!("Folder create failed: {path} ({e})")),
                }
                Task::none()
            }
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::OpenRevisionFinished { path, result },
            ) => {
                match result {
                    Ok(output_path) => screen
                        .push_log(format!("Opened archived revision: {path} -> {output_path}")),
                    Err(e) => {
                        screen.push_log(format!("Open archived revision failed: {path} ({e})"))
                    }
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn on_sync_actions(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_sync::OrgSyncScreen,
        actions: Vec<SyncAction>,
        root_dir: PathBuf,
        org_id: String,
        access_token: String
    ) -> Task<Message> {
        if actions.is_empty() {
            return Task::none();
        }

        let mut tasks = Vec::with_capacity(actions.len());
        for action in actions {
            match action {
                SyncAction::Move { from, to } => {
                    let object_id = state.index.get_file_id(&from);
                    let new_parent = to.parent().unwrap().to_path_buf();
                    let object_current_parent = from.parent().unwrap().to_path_buf();
                    let object_current_parent_id = state
                        .index
                        .get_file_id(&object_current_parent)
                        .unwrap()
                        .clone();
                    let new_parent_id = state.index.get_file_id(&new_parent);

                    if let Some(object_id) = object_id {
                        if let Some(new_parent_id) = new_parent_id {
                            let file_name = to.file_name().unwrap().to_string_lossy().to_string();

                            let move_future = DriveService::move_object(
                                object_id.clone(),
                                object_current_parent_id,
                                new_parent_id.clone(),
                                access_token.clone(),
                                file_name,
                            );
                            tasks.push(Task::perform(move_future, |result| {
                                Message::Sync(SyncMessage::ObjectMoved {
                                    from_path: from,
                                    to_path: to,
                                    result: result.map_err(SyncError::Common),
                                })
                            }));
                        } else {
                            println!("Missing data");
                        }
                    }
                }
                SyncAction::MoveAndUpload { from, to } | SyncAction::MoveFolder { from, to } => {
                    // TODO
                    println!("do something");
                }

                SyncAction::Upload(path) => {
                    println!("upload file");
                    if let Some(task) = Self::fs_upload(
                        &state.index,
                        path,
                        access_token.clone(),
                        org_id.clone(),
                        root_dir.clone(),
                    ) {
                        tasks.push(task);
                    }
                }
                SyncAction::Delete(path) => {
                    if let Some(skipped_path) =
                        Self::on_fs_delete_skip(state, path, root_dir.clone())
                    {
                        screen.push_log(format!(
                            "Skipped deleted file (no Drive delete): {}",
                            skipped_path.display()
                        ));
                    }
                }
                SyncAction::EnsureFolder(path) => {
                    if let Some(task) =
                        Self::on_fs_ensure_folder(state, screen, path, root_dir.clone(), org_id.clone(), access_token.clone())
                    {
                        tasks.push(task);
                    }
                }
                SyncAction::RemoveFolder(path) => {
                    if let Some(skipped_path) =
                        Self::on_fs_folder_delete_skip(state, path, root_dir.clone())
                    {
                        screen.push_log(format!(
                            "Skipped deleted folder (no Drive delete): {}",
                            skipped_path.display()
                        ));
                    }
                }
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(crate) async fn initial_sync(
        access_token: String,
        root_dir: PathBuf,
        root_dir_id: String,
    ) -> Result<FileIndex, SyncError> {
        let actions = Self::walk_directory_to_actions_bfs(root_dir.as_path());
        let mut file_index = FileIndex::default();
        file_index.put_file_id(root_dir, root_dir_id);

        let parent_and_paths: Vec<(PathBuf, PathBuf, bool)> = actions
            .into_iter()
            .map(|(path, is_folder)| (path.parent().unwrap().to_path_buf(), path, is_folder))
            .collect();

        for (parent, path, is_folder) in parent_and_paths {
            let parent_id = file_index.get_file_id(&parent).unwrap();
            let folder_name = path
                .file_name()
                .ok_or(SyncError::Common(
                    crate::app::message::CommonServiceError::Unknown(
                        "filename doesn't exist".into(),
                    ),
                ))?
                .to_string_lossy()
                .to_string();

            let id = if is_folder {
                let folder_id = DriveService::create_folder(
                    parent_id.as_str(),
                    &folder_name,
                    access_token.as_str(),
                )
                .await
                .map_err(SyncError::from)?;

                folder_id.id
            } else {
                let file_id = DriveService::upload_new_file(
                    path.clone(),
                    parent_id.to_string(),
                    access_token.clone(),
                )
                .await?;
                file_id.id
            };

            file_index.put_file_id(path, id);
        }

        Ok(file_index)
    }

    /// Search in fs_index if path exists, then upload to existing file
    /// Otherwise check if parent folder exists and upload a new file to it
    /// Otherwise upload all the folders up to parent (including) then send a new message SyncAction::Upload
    fn fs_upload(
        fs_index: &FileIndex,
        path: PathBuf,
        access_token: String,
        root_folder_id: String,
        root_dir: PathBuf,
    ) -> Option<Task<Message>> {
        if let Some(file_id) = fs_index.get_file_id(&path).cloned() {
            // upload by using the file id
            let future = DriveService::upload_existing_file(path.clone(), file_id, access_token);
            let task = Task::perform(future, |result| {
                Message::Sync(SyncMessage::UploadFinished { path, result })
            });
            return Some(task);
        }

        if let Some(parent_folder_id) = fs_index
            .get_file_id(&path.parent().unwrap().to_path_buf())
            .cloned()
        {
            let future =
                DriveService::upload_new_file(path.clone(), parent_folder_id, access_token);

            return Some(Task::perform(future, |result| {
                Message::Sync(SyncMessage::UploadFinished { path, result })
            }));
        }

        // try to find the parent and search in google drive for it
        let parent = path.parent().unwrap().to_path_buf();

        let prerequisite_task =
            DriveService::ensure_folder_on_remote(root_folder_id, root_dir, access_token, parent);

        prerequisite_task.map(|t| {
            t.chain(Task::done(Message::Sync(SyncMessage::ActionsReady(vec![
                SyncAction::Upload(path),
            ]))))
        })
    }

    fn on_fs_delete_skip(
        state: &mut crate::app::state::AppState,
        removed_path: PathBuf,
        root_dir: PathBuf,
    ) -> Option<PathBuf> {
        let mapped_root = root_dir;
        let removed_path = absolutize(&removed_path, &mapped_root);

        if removed_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.') || n == ".DS_Store")
        {
            return None;
        }

        if !is_path_under_root(&removed_path, &mapped_root) {
            return None;
        }

        let relative = match removed_path.strip_prefix(&mapped_root) {
            Ok(r) => r,
            Err(_) => return None,
        };

        let Some(_file_name) = relative.file_name().and_then(|s| s.to_str()) else {
            return None;
        };

        Some(removed_path)
    }

    fn on_fs_ensure_folder(
        state: &mut crate::app::state::AppState,
        screen: &mut screens::org_sync::OrgSyncScreen,
        folder_path: PathBuf,
        root_dir: PathBuf,
        org_id: String,
        access_token: String
    ) -> Option<Task<Message>> {

        DriveService::ensure_folder_on_remote(
            org_id,
            root_dir,
            access_token,
            folder_path,
        )
    }

    fn on_fs_folder_delete_skip(
        state: &mut crate::app::state::AppState,
        removed_path: PathBuf,
        root_dir: PathBuf,
    ) -> Option<PathBuf> {
        let mapped_root = root_dir;
        let removed_path = absolutize(&removed_path, &mapped_root);

        if removed_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.') || n == ".DS_Store")
        {
            return None;
        }

        if !is_path_under_root(&removed_path, &mapped_root) {
            return None;
        }

        let relative = match removed_path.strip_prefix(&mapped_root) {
            Ok(r) => r,
            Err(_) => return None,
        };

        relative.components().next()?;

        Some(removed_path)
    }

    pub fn open_revision_task(
        org_id: String,
        access_token: String,
        local_root: Option<String>,
        local_path: String,
        revision_id: String,
    ) -> Task<Message> {
        let local_path_clone = local_path.clone();
        Task::perform(
            async move {
                let root = local_root.ok_or_else(|| {
                    SyncError::InvalidLocalFolder(
                        "No local folder mapping found for this org.".to_string(),
                    )
                })?;
                let root_path = PathBuf::from(&root);
                let input_path = PathBuf::from(&local_path_clone);
                let absolute_path = absolutize(&input_path, &root_path);

                if !is_path_under_root(&absolute_path, &root_path) {
                    return Err(SyncError::InvalidLocalFolder(format!(
                        "Path is outside mapped folder: {}",
                        absolute_path.display()
                    )));
                }

                let relative_path = absolute_path
                    .strip_prefix(&root_path)
                    .map_err(|_| {
                        SyncError::InvalidLocalFolder(
                            "Unable to resolve relative path.".to_string(),
                        )
                    })?
                    .to_path_buf();

                let index = FileIndex::load();
                let file_id = index
                    .get_file_id(&relative_path)
                    .ok_or_else(|| {
                        SyncError::InvalidLocalFolder(
                            "No Drive file id found for this file.".to_string(),
                        )
                    })?
                    .to_string();

                let bytes = DriveService::download_revision(&file_id, &revision_id, &access_token)
                    .await
                    .map_err(SyncError::from)?;

                let output_path = build_archived_output_path(&absolute_path, &revision_id);
                std::fs::write(&output_path, bytes).map_err(|e| SyncError::Io(e.to_string()))?;

                open_local_file(&output_path)?;

                Ok(output_path.display().to_string())
            },
            move |result| {
                Message::Sync(SyncMessage::OpenRevisionFinished {
                    path: local_path,
                    result,
                })
            },
        )
    }

    // TODO: improve by returning levels of sync actions, that can be executed concurrently
    fn walk_directory_to_actions_bfs(root: &Path) -> Vec<(PathBuf, bool)> {
        let mut actions = Vec::new();
        let mut dirs_to_visit: VecDeque<PathBuf> = VecDeque::new();
        dirs_to_visit.push_front(root.to_path_buf());

        while let Some(dir) = dirs_to_visit.pop_back() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(rd) => rd,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let name_str = entry.file_name().to_string_lossy().to_string();

                if name_str.starts_with('.')
                    || name_str == ".DS_Store"
                    || name_str.contains(" (archived ")
                {
                    continue;
                }

                if path.is_dir() {
                    actions.push((path.clone(), true));
                    dirs_to_visit.push_front(path);
                } else if path.is_file() {
                    actions.push((path, false));
                }
            }
        }

        actions
    }
}

fn is_path_under_root(path: &Path, root: &Path) -> bool {
    let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path_abs = absolutize(path, root);
    path_abs.starts_with(root_abs)
}

fn absolutize(path: &Path, root: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn build_archived_output_path(original: &Path, revision_id: &str) -> PathBuf {
    let parent = original.parent().unwrap_or_else(|| Path::new("."));
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = original.extension().and_then(|s| s.to_str());

    let safe_revision = sanitize_filename_component(revision_id);
    let base = format!("{stem} (archived {safe_revision})");
    let mut candidate = match ext {
        Some(ext) => parent.join(format!("{base}.{ext}")),
        None => parent.join(base.clone()),
    };

    let mut counter = 2;
    while candidate.exists() {
        let numbered = format!("{base} {counter}");
        candidate = match ext {
            Some(ext) => parent.join(format!("{numbered}.{ext}")),
            None => parent.join(numbered.clone()),
        };
        counter += 1;
    }

    candidate
}

fn sanitize_filename_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn open_local_file(path: &Path) -> Result<(), SyncError> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .arg(path)
        .status()
        .map_err(|e| SyncError::Io(e.to_string()))?;

    #[cfg(not(target_os = "macos"))]
    let status = std::process::Command::new("xdg-open")
        .arg(path)
        .status()
        .map_err(|e| SyncError::Io(e.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(SyncError::Io("Failed to open file.".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{
        ArchiveClient,
        app::{
            message::SyncAction,
            state::{AppState, OrgState, SessionState, UserProfile},
        },
        screens::org_sync::OrgSyncScreen,
        services::{auth::AuthService, drive::DriveService, local_storage::LocalStorageService},
    };
}
