use std::path::PathBuf;

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgMessage, SyncError, SyncMessage},
    },
    services::{auth::AuthService, org::OrgService, resolver::Resolver},
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
                    .map_or(false, |object_was_on_remote| *object_was_on_remote),
                result: result.map(|_| ()),
            })
        })
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
