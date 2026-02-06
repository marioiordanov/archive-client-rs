use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{GlobalError, Message, OrgMessage},
        state::{Intent, Screen, SessionState},
    },
    screens::signin::SignInScreen,
    services::{auth::AuthService, org::OrgService},
};

impl ArchiveClient {
    pub fn re_auth(&mut self) -> Task<Message> {
        self.app.session = SessionState::default();
        self.screen = Screen::SignIn(SignInScreen::default());
        Task::none()
    }

    pub fn retry_intent(&self) -> Task<Message> {
        if let Some(intent) = self.app.retry_intent.as_ref() {
            self.run_intent(&intent)
        } else {
            Task::none()
        }
    }

    pub fn run_intent(&self, intent: &Intent) -> Task<Message> {
        let access_token = self.app.session.user.access_token.clone();
        match intent {
            Intent::FetchInvitations => {
                let email = self.app.session.user.email.clone();
                Self::fetch_invitations_task(email, access_token)
            }
            Intent::CreateOrg => Task::perform(
                OrgService::get_or_create_organization(
                    access_token,
                    self.app.session.user.email.clone(),
                ),
                |organisation| Message::Org(OrgMessage::OrgCreated(organisation)),
            ),
            Intent::SendInvitations {
                run_id,
                email,
                org_id,
            } => Self::invite_user_task(*run_id, email.clone(), org_id.clone(), access_token),
            Intent::LoadDashboard { org_id } => {
                Self::load_dashboard_task(org_id.clone(), access_token)
            }
        }
    }

    pub fn handle_error(&mut self, error: GlobalError) -> Task<Message> {
        match error {
            GlobalError::Common(app::message::CommonServiceError::TokenExpired) => {
                if !self.app.is_signed_in() {
                    self.re_auth()
                } else {
                    let refresh_token = self.app.session.user.refresh_token.clone();

                    Task::perform(
                        async move { AuthService::refresh_access_token(&refresh_token).await },
                        |response| {
                            Message::Auth(app::message::AuthMessage::AccessTokenRefreshed(response))
                        },
                    )
                }
            }
            _ => Task::none(),
        }
    }

    // changes the state, switch screen if needed
    pub fn update(&mut self, message: Message) -> Task<Message> {
        println!("{message:?}");
        self.handle_message(message).0
    }
}
