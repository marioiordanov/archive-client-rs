use iced::Task;
use log::warn;

use crate::{
    ArchiveClient, UserState,
    app::{
        self,
        message::{GlobalError, Message, OrgMessage, UnixSocketCommand},
        state::{Intent, Screen},
    },
    screens::signin::SignInScreen,
    services::{auth::AuthService, org::OrgService},
};

impl ArchiveClient {
    pub fn re_auth(&mut self) -> Task<Message> {
        self.app.user_state.sign_out();
        self.screen = Screen::SignIn(SignInScreen::default());
        Task::none()
    }

    pub fn retry_intents(&mut self) -> Task<Message> {
        let intents = self.app.pending_intents.drain(..).collect::<Vec<Intent>>();
        let tasks: Vec<Task<Message>> = intents
            .into_iter()
            .map(|intent| self.run_intent(intent))
            .collect();

        Task::batch(tasks)
    }

    pub fn run_intent(&self, intent: Intent) -> Task<Message> {
        match (&self.app.user_state, intent) {
            (UserState::SignedIn { user_data }, Intent::FetchInvitations) => {
                Self::fetch_invitations_task(
                    user_data.email.clone(),
                    user_data.access_token.clone(),
                )
            }
            (UserState::SignedIn { user_data }, Intent::CreateOrg) => Task::perform(
                OrgService::get_or_create_organization(
                    user_data.access_token.clone(),
                    user_data.email.clone(),
                ),
                |organization| Message::Org(OrgMessage::OrgCreated(organization)),
            ),
            (
                UserState::OrgCreated { user_data, .. },
                Intent::SendInvitations {
                    run_id,
                    email,
                    org_id,
                },
            ) => Self::invite_user_task(
                run_id,
                email.clone(),
                org_id.clone(),
                user_data.access_token.clone(),
            ),
            (UserState::OrgCreated { user_data, .. }, Intent::LoadDashboard { org_id }) => {
                Self::load_dashboard_task(org_id.clone(), user_data.access_token.clone())
            }
            (
                UserState::OrgJoined {
                    user_data,
                    root_folder_id,
                },
                Intent::InitialSync { root_dir },
            ) => Self::initial_sync_task(
                user_data.access_token.clone(),
                root_dir,
                root_folder_id.clone(),
            ),
            (
                UserState::OrgSynced {
                    resolver,
                    root_folder_id,
                    root_dir,
                    user_data,
                },
                Intent::Upload { path },
            ) => Self::upload_task(
                path,
                root_dir.clone(),
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
            ),
            (
                UserState::OrgSynced {
                    resolver,
                    root_folder_id,
                    root_dir,
                    user_data,
                },
                Intent::EnsureFolder { path },
            ) => Self::ensure_folder_task(
                path,
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
            ),
            (
                UserState::OrgSynced {
                    resolver,
                    root_folder_id,
                    root_dir,
                    user_data,
                },
                Intent::Move { from, to },
            ) => Self::move_task(
                from,
                to,
                root_dir.clone(),
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
            ),
            (
                UserState::OrgSynced {
                    resolver,
                    root_folder_id,
                    root_dir,
                    user_data,
                },
                Intent::MoveAndUpload { from, to },
            ) => Self::move_then_upload_task(
                from,
                to,
                root_dir.clone(),
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
            ),
            (
                UserState::OrgSynced {
                    resolver,
                    root_folder_id,
                    root_dir,
                    user_data,
                },
                Intent::Remove { path },
            ) => Self::delete_task(
                path,
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
            ),
            (
                UserState::OrgSynced {
                    resolver,
                    root_folder_id,
                    root_dir,
                    user_data,
                },
                Intent::ExternalRequest { cmd },
            ) => match cmd {
                UnixSocketCommand::GetFileRevisions { path, sender } => {
                    Self::get_file_revisions_task(
                        path,
                        sender,
                        root_folder_id.clone(),
                        resolver.clone(),
                        user_data.access_token.clone(),
                    )
                }
                UnixSocketCommand::DownloadFileAtPath { file_id, revision_id, modified_time, sender } => {
                    Self::download_file_at_path_task(
                        file_id,
                        revision_id,
                        modified_time,
                        resolver.clone(),
                        root_dir.clone(),
                        user_data.access_token.clone(),
                        sender,
                    )
                }
                _ => Task::none(),
            },
            (user_state, intent) => {
                warn!("impossible combination {user_state} {intent:?}");
                Task::none()
            }
        }
    }

    pub fn handle_error(&mut self, error: GlobalError) -> Task<Message> {
        match error {
            GlobalError::Common(app::message::CommonServiceError::TokenExpired(expired_token)) => {
                match &self.app.user_state {
                    crate::UserState::SignedOut => self.re_auth(),
                    crate::UserState::SignedIn { user_data }
                    | crate::UserState::OrgCreated { user_data, .. }
                    | crate::UserState::OrgJoined { user_data, .. }
                    | crate::UserState::OrgSynced { user_data, .. } => {
                        if user_data.access_token == expired_token {
                            if self.app.pending_refresh {
                                Task::none()
                            } else {
                                let refresh_token = user_data.refresh_token.clone();
                                self.app.pending_refresh = true;
                                Task::perform(
                                    async move {
                                        AuthService::refresh_access_token(&refresh_token).await
                                    },
                                    |response| {
                                        Message::Auth(
                                            app::message::AuthMessage::AccessTokenRefreshed(
                                                response,
                                            ),
                                        )
                                    },
                                )
                            }
                        } else {
                            Task::none()
                        }
                    }
                }
            }
            _ => Task::none(),
        }
    }

    // changes the state, switch screen if needed
    pub fn update(&mut self, message: Message) -> Task<Message> {
        self.handle_message(message)
    }
}
