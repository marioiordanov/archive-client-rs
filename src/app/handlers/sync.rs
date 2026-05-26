use std::{
    collections::{HashMap, VecDeque},
    future,
    path::{Path, PathBuf},
    sync::Arc,
};

use iced::{
    Task,
    futures::{StreamExt, stream},
};

use crate::{
    ArchiveClient, UserState,
    app::{
        handlers::org,
        message::{CommonServiceError, Message, SyncAction, SyncError, SyncMessage},
        state::{Intent, OrgConfig, OrgState, Screen, UserProfile},
    },
    screens,
    services::{
        drive::{DriveFile, DriveService},
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
                SyncMessage::InitialSyncCompleted { root_dir, result },
            ) => match result {
                Ok(file_index) => {
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
                Err(e) => {
                    if matches!(e, SyncError::Common(CommonServiceError::TokenExpired(..))) {
                        self.app
                            .pending_intents
                            .push(Intent::InitialSync { root_dir });
                    }
                    self.handle_error(e.into())
                }
            },
            (
                UserState::OrgSynced {
                    root_dir,
                    root_folder_id,
                    user_data,
                    resolver,
                    ..
                },
                Screen::OrgSync(screen),
                SyncMessage::ActionsReady(actions),
            ) => {
                let root_dir = root_dir.clone();
                let org_id = root_folder_id.clone();
                let access_token = user_data.access_token.clone();
                Self::on_sync_actions(
                    screen,
                    actions,
                    root_dir,
                    org_id,
                    access_token,
                    resolver.clone(),
                )
            }
            (
                UserState::OrgSynced { resolver, .. },
                Screen::OrgSync(screen),
                SyncMessage::UploadFinished { path, result },
            ) => match result {
                Ok(..) => {
                    screen.push_log(format!("Uploaded: {}", path.display()));
                    Task::none()
                }
                Err(e) => {
                    match e {
                        SyncError::Common(CommonServiceError::TokenExpired(..)) => {
                            self.app
                                .pending_intents
                                .push(Intent::Upload { path: path.clone() });
                            screen.push_log(format!("Retry upload {}", path.display()));
                        }
                        _ => {
                            screen.push_log(format!("Upload failed: {} ({e})", path.display()));
                        }
                    }

                    self.handle_error(e.into())
                }
            },
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::FolderEnsureFinished {
                    path,
                    result: Err(e),
                },
            ) => {
                match e {
                    SyncError::Common(CommonServiceError::TokenExpired(..)) => {
                        self.app.pending_intents.push(Intent::EnsureFolder { path });
                    }
                    _ => {
                        screen.push_log(format!("Folder archive failed: {} ({e})", path.display()));
                    }
                }
                self.handle_error(e.into())
            }
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::MoveFinished {
                    from_path,
                    to_path,
                    result,
                },
            ) => match result {
                Ok(_) => {
                    screen.push_log(format!(
                        "Renamed from {} to {}",
                        from_path.display(),
                        to_path.display()
                    ));
                    Task::none()
                }
                Err(e) => {
                    if matches!(e, SyncError::Common(CommonServiceError::TokenExpired(..))) {
                        self.app.pending_intents.push(Intent::Move {
                            from: from_path.clone(),
                            to: to_path.clone(),
                        });
                        screen.push_log(format!(
                            "Retry rename {} to {}",
                            from_path.display(),
                            to_path.display()
                        ));
                    } else {
                        screen.push_log(format!(
                            "Renamed failed from {} to {}",
                            from_path.display(),
                            to_path.display()
                        ));
                    }
                    self.handle_error(e.into())
                }
            },
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::MoveThenUploadFinished { from, to, result },
            ) => match result {
                Ok(_) => {
                    screen.push_log(format!(
                        "Renamed from {} to {} and uploaded",
                        from.display(),
                        to.display()
                    ));
                    Task::none()
                }
                Err(e) => {
                    if matches!(e, SyncError::Common(CommonServiceError::TokenExpired(..))) {
                        self.app.pending_intents.push(Intent::MoveAndUpload {
                            from: from.clone(),
                            to: to.clone(),
                        });
                        screen.push_log(format!(
                            "Retry rename and upload {} to {}",
                            from.display(),
                            to.display()
                        ));
                    } else {
                        screen.push_log(format!(
                            "Renamed and upload failed from {} to {}",
                            from.display(),
                            to.display()
                        ));
                    }
                    self.handle_error(e.into())
                }
            },
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::RemoveFinished {
                    path,
                    object_was_on_remote: true,
                    result: Ok(..),
                },
            ) => {
                screen.push_log(format!("Removed {}", path.display(),));
                Task::none()
            }
            (
                UserState::OrgSynced { .. },
                Screen::OrgSync(screen),
                SyncMessage::RemoveFinished {
                    path,
                    object_was_on_remote,
                    result: Err(e),
                },
            ) => {
                if matches!(e, SyncError::Common(CommonServiceError::TokenExpired(..))) {
                    self.app
                        .pending_intents
                        .push(Intent::Remove { path: path.clone() });
                    screen.push_log(format!("Retry remove {}", path.display(),));
                } else {
                    screen.push_log(format!("Remove failed {}", path.display(),));
                }
                self.handle_error(e.into())
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

    /// Deletes the Drive object at `path` if it exists on remote.
    ///
    /// Returns `true` if the object was found and deleted, `false` if it didn't exist on Drive.
    /// Any other error (network, permissions, etc.) is propagated.
    pub(crate) async fn delete_object_if_on_remote(
        path: PathBuf,
        resolver: Resolver,
        root_dir_id: String,
        access_token: String,
    ) -> Result<bool, SyncError> {
        let object_id_result = resolver
            .resolve_path(path.clone(), root_dir_id, access_token.clone())
            .await;

        let object_was_on_remote = match object_id_result {
            Ok(object_id) => DriveService::delete_object(object_id, access_token)
                .await
                .map_err(SyncError::from)
                .map(|_| true),
            Err(SyncError::PathDoesNotExistOnRemote(_)) => Ok(false),
            Err(err) => Err(err),
        }?;

        resolver.remove_from_file_index(path).await;

        Ok(object_was_on_remote)
    }

    fn on_sync_actions(
        screen: &mut screens::org_sync::OrgSyncScreen,
        actions: Vec<SyncAction>,
        root_dir: PathBuf,
        org_id: String,
        access_token: String,
        resolver: Resolver,
    ) -> Task<Message> {
        if actions.is_empty() {
            return Task::none();
        }

        let mut tasks = Vec::with_capacity(actions.len());
        for action in actions {
            match action {
                SyncAction::Move { from, to } | SyncAction::MoveFolder { from, to } => {
                    tasks.push(Self::move_task(
                        from.clone(),
                        to.clone(),
                        root_dir.clone(),
                        org_id.clone(),
                        resolver.clone(),
                        access_token.clone(),
                    ));
                }
                SyncAction::MoveAndUpload { from, to } => {
                    tasks.push(Self::move_then_upload_task(
                        from.clone(),
                        to.clone(),
                        root_dir.clone(),
                        org_id.clone(),
                        resolver.clone(),
                        access_token.clone(),
                    ));
                }

                SyncAction::Upload(path) => {
                    tasks.push(Self::upload_task(
                        path,
                        root_dir.clone(),
                        org_id.clone(),
                        resolver.clone(),
                        access_token.clone(),
                    ));
                }
                SyncAction::Delete(path) | SyncAction::RemoveFolder(path) => {
                    tasks.push(Self::delete_task(
                        path,
                        org_id.clone(),
                        resolver.clone(),
                        access_token.clone(),
                    ));
                }
                SyncAction::EnsureFolder(path) => {
                    tasks.push(Self::ensure_folder_task(
                        path,
                        org_id.clone(),
                        resolver.clone(),
                        access_token.clone(),
                    ));
                }
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            // TODO: improve by grouping tasks that can be executed in parallel
            tasks
                .into_iter()
                .reduce(|acc, task| acc.chain(task))
                .unwrap_or(Task::none())
                .chain(Task::perform(
                    async move { resolver.save_on_local().await },
                    |s| (Message::Sync(SyncMessage::BatchCompleted)),
                ))
        }
    }

    pub(crate) async fn move_object(
        from: PathBuf,
        to: PathBuf,
        resolver: Resolver,
        root_dir: PathBuf,
        root_dir_id: String,
        access_token: String,
    ) -> Result<String, SyncError> {
        let from_id_result = resolver
            .resolve_path(from.clone(), root_dir_id.clone(), access_token.clone())
            .await;

        match from_id_result {
            Ok(from_id) => {
                let new_name = to.file_name().unwrap().to_string_lossy().to_string();
                let old_parent = from.parent().unwrap().to_path_buf();
                let old_parent_id = resolver
                    .resolve_path(
                        old_parent.clone(),
                        root_dir_id.clone(),
                        access_token.clone(),
                    )
                    .await?;

                let new_parent = to.parent().unwrap().to_path_buf();
                let new_parent_id = resolver
                    .resolve_and_create_missing_ancestors(
                        new_parent,
                        root_dir_id.clone(),
                        access_token.clone(),
                    )
                    .await?;

                let file_id = DriveService::move_object(
                    from_id,
                    old_parent_id,
                    new_parent_id,
                    access_token,
                    new_name,
                )
                .await
                .map(|drive_file| drive_file.id)?;

                resolver
                    .move_object_in_file_index(from, to, file_id.clone())
                    .await;

                Ok(file_id)
            }
            Err(SyncError::PathDoesNotExistOnRemote(..)) => {
                let file_id = Self::upload(
                    resolver.clone(),
                    to.clone(),
                    root_dir,
                    root_dir_id,
                    access_token,
                )
                .await?;
                resolver.update_file_index(to, file_id.clone()).await;
                Ok(file_id)
            }
            Err(err) => Err(err),
        }
    }

    pub(crate) async fn initial_sync(
        access_token: String,
        root_dir: PathBuf,
        root_dir_id: String,
    ) -> Result<FileIndex, SyncError> {
        let actions = Self::walk_directory_to_actions_bfs(root_dir.as_path());
        let mut file_index = FileIndex::new(root_dir, root_dir_id);

        let parent_and_paths: Vec<(PathBuf, PathBuf, bool)> = actions
            .into_iter()
            .map(|(path, is_folder)| (path.parent().unwrap().to_path_buf(), path, is_folder))
            .collect();

        for (parent, path, is_folder) in parent_and_paths {
            let parent_id = file_index.get_file_id(parent).unwrap();
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

    pub(crate) async fn upload(
        resolver: Resolver,
        path: PathBuf,
        root_dir: PathBuf,
        root_dir_id: String,
        access_token: String,
    ) -> Result<String, SyncError> {
        let resolve_path_result = resolver
            .resolve_and_create_missing_ancestors(
                path.clone(),
                root_dir_id.clone(),
                access_token.clone(),
            )
            .await;

        match resolve_path_result {
            Ok(file_id) => {
                let drive_file =
                    DriveService::upload_existing_file(path.clone(), file_id, access_token).await?;
                resolver
                    .update_file_index(path, drive_file.id.clone())
                    .await;
                Ok(drive_file.id)
            }
            Err(SyncError::PathDoesNotExistOnRemote(non_existing_path))
                if non_existing_path.eq(&path) =>
            {
                let parent_id = resolver
                    .resolve_path(
                        path.parent().unwrap().to_path_buf(),
                        root_dir_id,
                        access_token.clone(),
                    )
                    .await?;
                let file_id =
                    DriveService::upload_new_file(path.clone(), parent_id, access_token).await?;
                resolver
                    .update_file_index(path.clone(), file_id.id.clone())
                    .await;
                Ok(file_id.id)
            }
            Err(e) => Err(e),
        }
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

                let index = FileIndex::load(org_id);
                let file_id = index
                    .get_file_id(relative_path)
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
                    || name_str.contains(" .archived ")
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
        services::{
            auth::AuthService, drive::DriveService, file_index::FileIndex,
            local_storage::LocalStorageService, resolver::Resolver,
        },
    };

    #[tokio::test]
    async fn check_upload_file() {
        //let refresh = AuthService::refresh_access_token("REFRESH_TOKEN".into()).await.unwrap();

        let root_dir = "/Users/mario/Projects/archive-client-rs/test-folder";
        let path = "/Users/mario/Projects/archive-client-rs/test-folder/mario/aaaa/x";
        let resolver = Resolver::new(root_dir.into(), FileIndex::default());

        let r = ArchiveClient::upload(
            resolver,
            path.into(),
            root_dir.into(),
            "18NTDkndn_ESjsActq-6CRFUiMvfHTLWL".into(),
            "ACCESS_TOKEN".into(),
        )
        .await;

        println!("{r:?}");
    }

    #[tokio::test]
    async fn check_some_actions() {
        let refresh = AuthService::refresh_access_token("REFRESH_TOKEN".into()).await.unwrap();
        let mut screen = OrgSyncScreen::new(None);
        let root_dir: PathBuf = "/Users/mario/Projects/archive-client-rs/test-folder".into();
        let root_dir_id = "18NTDkndn_ESjsActq-6CRFUiMvfHTLWL".to_string();
        let resolver = Resolver::new(root_dir.clone(), FileIndex::load(root_dir_id.clone()));

        let r = ArchiveClient::initial_sync(refresh.access_token.clone(), root_dir.clone(), root_dir_id.clone()).await.unwrap();

        let resolver = Resolver::new(root_dir, r);
        let object_id = resolver.resolve_path("/Users/mario/Projects/archive-client-rs/test-folder/mario1".into(), root_dir_id, refresh.access_token).await.unwrap();

        resolver.move_object_in_file_index("/Users/mario/Projects/archive-client-rs/test-folder/mario1".into(), "/Users/mario/Projects/archive-client-rs/test-folder/mario".into(), object_id).await;
        resolver.save_on_local().await;
    }
}
