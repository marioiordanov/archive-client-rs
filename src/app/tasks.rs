use std::path::PathBuf;

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgMessage},
    },
    services::{auth::AuthService, org::OrgService},
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
        let future = ArchiveClient::initial_sync(access_token, root_dir, root_dir_id);
        Task::perform(future, |result| {
            Message::Sync(app::message::SyncMessage::InitialSyncCompleted(result))
        })
    }
}
