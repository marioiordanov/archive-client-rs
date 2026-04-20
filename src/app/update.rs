use iced::Task;
use log::warn;

use crate::{
    ArchiveClient, UserState,
    app::{
        self,
        message::{GlobalError, Message, OrgMessage},
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

    pub fn retry_intent(&self) -> Task<Message> {
        if let Some(intent) = self.app.retry_intent.as_ref() {
            self.run_intent(intent)
        } else {
            Task::none()
        }
    }

    pub fn run_intent(&self, intent: &Intent) -> Task<Message> {
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
                *run_id,
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
                root_dir.clone(),
                root_folder_id.clone(),
            ),
            (user_state, intent) => {
                warn!("impossible combination {user_state} {intent:?}");
                Task::none()
            }
        }
    }

    pub fn handle_error(&mut self, error: GlobalError) -> Task<Message> {
        match error {
            GlobalError::Common(app::message::CommonServiceError::TokenExpired) => {
                match &self.app.user_state {
                    crate::UserState::SignedOut => self.re_auth(),
                    crate::UserState::SignedIn { user_data }
                    | crate::UserState::OrgCreated { user_data, .. }
                    | crate::UserState::OrgJoined { user_data, .. }
                    | crate::UserState::OrgSynced { user_data, .. } => {
                        let refresh_token = user_data.refresh_token.clone();

                        Task::perform(
                            async move { AuthService::refresh_access_token(&refresh_token).await },
                            |response| {
                                Message::Auth(app::message::AuthMessage::AccessTokenRefreshed(
                                    response,
                                ))
                            },
                        )
                    }
                }
            }
            _ => Task::none(),
        }
    }

    // changes the state, switch screen if needed
    pub fn update(&mut self, message: Message) -> Task<Message> {
        println!("{message:?}");
        self.handle_message(message)
    }
}
