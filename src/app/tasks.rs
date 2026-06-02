use std::{path::PathBuf, task::Poll};

use iced::{Task, futures::{ poll}};

use crate::{
    ArchiveClient,
    app::{
        self,
        handlers::external_commands::FileWithRevision,
        message::{
            CommonServiceError, LoadingRevisions, Message, OrgMessage, SyncError,
            SyncMessage, UnixSocketCommand,
        },
    },
    services::{
        auth::AuthService,
        drive::{ DriveService},
        org::OrgService,
        resolver::Resolver,
        revisions_cache::Cache,
    },
};

impl ArchiveClient {
    pub fn get_access_token_task() -> Task<Message> {
        Task::perform(AuthService::get_drive_access_token(), |access_token| {
            Message::Auth(app::message::AuthMessage::AccessTokenReceived(access_token))
        })
    }
    pub fn get_or_create_organization_task(
        user_email: String,
        access_token: String,
    ) -> Task<Message> {
        Task::perform(
            OrgService::get_or_create_organization(access_token, user_email),
            |organization| Message::Org(OrgMessage::OrgCreated(organization)),
        )
    }

    pub fn fetch_invitations_task(user_email: String, access_token: String) -> Task<Message> {
        Task::perform(
            async move { OrgService::fetch_invitations(&user_email, &access_token).await },
            |result| Message::Org(OrgMessage::InvitationsLoaded(result)),
        )
    }

    pub fn invite_user_task(
        run_id: u64,
        email: String,
        org_id: String,
        access_token: String,
    ) -> Task<Message> {
        let email_for_async = email.clone();
        Task::perform(
            async move {
                OrgService::invite_user(&email_for_async, &org_id, &access_token)
                    .await
                    .map(|r| (r.0.id, r.1))
            },
            move |result| {
                Message::Org(OrgMessage::InviteUserFinished {
                    run_id,
                    email,
                    result,
                })
            },
        )
    }

    pub fn load_dashboard_task(org_id: String, access_token: String) -> Task<Message> {
        Task::perform(
            async move { OrgService::load_dashboard(&org_id, &access_token).await },
            |result| Message::Org(OrgMessage::DashboardLoaded(result)),
        )
    }

    pub fn fetch_audit_log_task(
        org_folder_id: String,
        access_token: String,
        page_token: Option<String>,
    ) -> Task<Message> {
        Task::perform(
            async move {
                DriveService::fetch_activity_log(&org_folder_id, &access_token, page_token)
                    .await
                    .map_err(app::message::OrgError::from)
            },
            |result| Message::Org(OrgMessage::AuditLogLoaded { result }),
        )
    }

    pub fn revoke_permission_task(
        folder_id: String,
        email: String,
        permission_id: Option<String>,
        access_token: String,
    ) -> Task<Message> {
        let folder_id_for_async = folder_id.clone();
        Task::perform(
            async move {
                OrgService::revoke_user_folder_permission(
                    &folder_id_for_async,
                    &email,
                    permission_id.as_deref(),
                    &access_token,
                )
                .await
            },
            move |result| Message::Org(OrgMessage::PermissionRevoked { folder_id, result }),
        )
    }

    pub fn initial_sync_task(
        access_token: String,
        root_dir: PathBuf,
        root_dir_id: String,
    ) -> Task<Message> {
        let future = ArchiveClient::initial_sync(access_token, root_dir.clone(), root_dir_id);
        Task::perform(future, |result| {
            Message::Sync(app::message::SyncMessage::InitialSyncCompleted { result, root_dir })
        })
    }

    pub fn upload_task(
        path: PathBuf,
        root_dir: PathBuf,
        root_dir_id: String,
        resolver: Resolver,
        access_token: String,
    ) -> Task<Message> {
        let async_call = Self::upload(resolver, path.clone(), root_dir, root_dir_id, access_token);
        Task::perform(async_call, |result| {
            Message::Sync(SyncMessage::UploadFinished { path, result })
        })
    }

    pub fn move_task(
        from: PathBuf,
        to: PathBuf,
        root_dir: PathBuf,
        root_dir_id: String,
        resolver: Resolver,
        access_token: String,
    ) -> Task<Message> {
        let move_async_call = Self::move_object(
            from.clone(),
            to.clone(),
            resolver,
            root_dir,
            root_dir_id,
            access_token,
        );
        Task::perform(move_async_call, |result| {
            Message::Sync(SyncMessage::MoveFinished {
                from_path: from,
                to_path: to,
                result,
            })
        })
    }

    pub fn move_then_upload_task(
        from: PathBuf,
        to: PathBuf,
        root_dir: PathBuf,
        root_dir_id: String,
        resolver: Resolver,
        access_token: String,
    ) -> Task<Message> {
        let move_async_call = Self::move_object(
            from.clone(),
            to.clone(),
            resolver.clone(),
            root_dir.clone(),
            root_dir_id.clone(),
            access_token.clone(),
        );

        let upload_async_call =
            Self::upload(resolver, to.clone(), root_dir, root_dir_id, access_token);

        Task::perform(
            async move {
                move_async_call.await?;
                upload_async_call.await
            },
            |result| Message::Sync(SyncMessage::MoveThenUploadFinished { from, to, result }),
        )
    }

    pub fn delete_task(
        path: PathBuf,
        root_dir_id: String,
        resolver: Resolver,
        access_token: String,
    ) -> Task<Message> {
        let async_call =
            Self::delete_object_if_on_remote(path.clone(), resolver, root_dir_id, access_token);
        Task::perform(async_call, |result: Result<bool, SyncError>| {
            Message::Sync(SyncMessage::RemoveFinished {
                path,
                object_was_on_remote: result
                    .as_ref()
                    .is_ok_and(|object_was_on_remote| *object_was_on_remote),
                result: result.map(|_| ()),
            })
        })
    }
    pub fn show_all_revisions_task(
        path: PathBuf,
        root_folder_id: String,
        resolver: Resolver,
        access_token: String,
        cache: Cache,
    ) -> Task<Message> {
        let revisions_future = Self::get_file_revisions(path.clone(), true, None, root_folder_id, resolver, access_token, cache);
        Task::perform(
                revisions_future
            ,
            |result: Result<Vec<FileWithRevision>, CommonServiceError>| match result {
                Ok(revisions) => Message::Sync(SyncMessage::AllRevisionsLoaded(revisions)),
                Err(e) => Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted {
                    command: Some(Box::new(UnixSocketCommand::ShowAllRevisions { path })),
                    error: Some(e),
                }),
            },
        )
    }

    async fn fetch_and_cache_file_revisions(
        file_id: String,
        force_refresh: bool,
        access_token: String,
        cache: Cache,
    ) -> Result<Vec<FileWithRevision>, CommonServiceError> {
        let revisions = match cache.get(file_id.clone()) {
            Some(revisions) if !force_refresh => revisions,
            _ => {
                let mut fetched = DriveService::list_revisions(&file_id, &access_token).await?;
                fetched.sort_by_key(|r| r.modified_time);
                fetched.reverse();
                cache.insert(file_id.clone(), fetched.clone());

                fetched
            }
        };

        let revisions = revisions
            .into_iter()
            .map(|r| FileWithRevision::new(file_id.clone(), r))
            .collect();

        Ok(revisions)
    }

    pub async fn get_file_revisions(
        path: PathBuf,
        force_refresh: bool,
        sender_option: Option<Box<tokio::sync::oneshot::Sender<LoadingRevisions>>>,
        root_folder_id: String,
        resolver: Resolver,
        access_token: String,
        cache: Cache,
    ) -> Result<Vec<FileWithRevision>, CommonServiceError> {
        let id_result_future =
            resolver.resolve_path(path.clone(), root_folder_id, access_token.clone());

        // TODO: use this syntax
        // let c = resolver.resolve_path(path.clone(), root_folder_id, access_token.clone())
        // .map_err(|e| match e {
        //             SyncError::Common(c) => c,
        //             o => CommonServiceError::Unknown(o.to_string()),
        //         }).and_then(|id| async move {Self::fetch_file_revisions(id, force_refresh, access_token, cache).await});

        tokio::pin!(id_result_future);

        match poll!(&mut id_result_future) {
            Poll::Pending => {
                if let Some(sender) = sender_option {
                    let _ = sender.send(LoadingRevisions::Loading);
                }
                let id = id_result_future.await.map_err(|e| match e {
                    SyncError::Common(c) => c,
                    o => CommonServiceError::Unknown(o.to_string()),
                })?;

                Self::fetch_and_cache_file_revisions(id, force_refresh, access_token, cache).await
            }
            Poll::Ready(Err(e)) => {
                if let Some(sender) = sender_option {
                    if let SyncError::Common(CommonServiceError::TokenExpired(..)) = e {
                        let _ = sender.send(LoadingRevisions::Loading);
                    } else {
                        let _ = sender.send(LoadingRevisions::Error);
                    }
                }

                let common_error = match e {
                    SyncError::Common(c) => c,
                    o => CommonServiceError::Unknown(o.to_string()),
                };

                Err(common_error)
            }
            Poll::Ready(Ok(id)) => {
                let revisions_result_future =
                    Self::fetch_and_cache_file_revisions(id, force_refresh, access_token, cache);
                tokio::pin!(revisions_result_future);

                match poll!(&mut revisions_result_future) {
                    Poll::Ready(Ok(revisions)) => {
                        if let Some(sender) = sender_option {
                            let _ = sender.send(LoadingRevisions::Loaded(revisions.clone()));
                        }

                        Ok(revisions)
                    }
                    Poll::Ready(Err(e)) => {
                        if let Some(sender) = sender_option {
                            if let CommonServiceError::TokenExpired(..) = e {
                                let _ = sender.send(LoadingRevisions::Loading);
                            } else {
                                let _ = sender.send(LoadingRevisions::Error);
                            }
                        }

                        Err(e)
                    }
                    Poll::Pending => {
                        if let Some(sender) = sender_option {
                            let _ = sender.send(LoadingRevisions::Loading);
                        }
                        revisions_result_future.await
                    }
                }
            }
        }
    }

    pub fn get_file_revisions_task(
        path: PathBuf,
        force_refresh: bool,
        sender_option: Option<Box<tokio::sync::oneshot::Sender<LoadingRevisions>>>,
        root_folder_id: String,
        resolver: Resolver,
        access_token: String,
        cache: Cache,
    ) -> Task<Message> {
        let path_for_retry = path.clone();
        let get_revisions_future = Self::get_file_revisions(
            path,
            force_refresh,
            sender_option,
            root_folder_id,
            resolver,
            access_token,
            cache,
        );
        Task::perform(
            async move { get_revisions_future.await },
            move |result| match result {
                Ok(_) => Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted {
                    command: None,
                    error: None,
                }),
                Err(err) => Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted {
                    command: Some(Box::new(UnixSocketCommand::GetFileRevisions {
                        path: path_for_retry,
                        force_refresh,
                        sender: None,
                    })),
                    error: Some(err),
                }),
            },
        )
    }

    pub fn download_file_at_path_task(
        file_id: String,
        revision_id: String,
        modified_time: String,
        resolver: Resolver,
        root_dir: PathBuf,
        access_token: String,
        sender: Box<tokio::sync::oneshot::Sender<String>>,
    ) -> Task<Message> {
        Task::perform(
            async move {
                if let Some(file_name) = resolver.get_object_name(&file_id).await {
                    let file_contents = match DriveService::download_revision(
                        &file_id,
                        &revision_id,
                        &access_token,
                    )
                    .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            println!("err downloading revision");
                            return Err((
                                e,
                                Box::new(UnixSocketCommand::DownloadFileAtPath {
                                    file_id,
                                    revision_id,
                                    modified_time,
                                    sender,
                                }),
                            ));
                        }
                    };

                    let modified_time_for_path = if cfg!(windows) {
                        modified_time.replace(':', "-")
                    } else {
                        modified_time.clone()
                    };

                    let file_name = if let Some(extension_idx) = file_name.rfind('.') {
                        format!(
                            "{} {}{}",
                            file_name[..extension_idx].to_string(),
                            modified_time_for_path,
                            file_name[extension_idx..].to_string()
                        )
                    } else {
                        format!("{file_name} {modified_time_for_path}")
                    };

                    let parent = root_dir.join(".archived");
                    if !parent.exists() {
                        let _ = tokio::fs::create_dir(&parent).await;
                    }

                    let file_path = parent.join(&file_name);
                    if let Err(e) = tokio::fs::write(&file_path, file_contents).await {
                        return Err((
                            CommonServiceError::Unknown(e.to_string()),
                            Box::new(UnixSocketCommand::DownloadFileAtPath {
                                file_id,
                                revision_id,
                                modified_time,
                                sender,
                            }),
                        ));
                    }

                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        let _ = std::process::Command::new("explorer")
                            .raw_arg(format!("/select,{}", file_path.display()))
                            .spawn();
                    }
                    #[cfg(not(windows))]
                    let _ = std::process::Command::new("open")
                        .args(["-R", &file_path.to_string_lossy()])
                        .spawn();
                    let _ = sender.send(file_path.to_string_lossy().into_owned());
                }
                Ok(())
            },
            |result| match result {
                Ok(_) => Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted {
                    command: None,
                    error: None,
                }),
                Err((err, cmd)) => Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted {
                    command: Some(cmd),
                    error: Some(err),
                }),
            },
        )
    }

    pub fn ensure_folder_task(
        path: PathBuf,
        root_dir_id: String,
        resolver: Resolver,
        access_token: String,
    ) -> Task<Message> {
        let path_for_task = path.clone();
        Task::perform(
            async move {
                resolver
                    .resolve_and_create_missing_ancestors(path_for_task, root_dir_id, access_token)
                    .await
                    .map(|_| ())
            },
            |result| Message::Sync(SyncMessage::FolderEnsureFinished { path, result }),
        )
    }
}
