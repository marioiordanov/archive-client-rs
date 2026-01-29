use std::time::{SystemTime, UNIX_EPOCH};

use iced::{Task, futures::FutureExt};

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{AuthError, GlobalError, Message, OrgMessage, ScreenMessage},
        state::{AuthState, Intent, Screen, SessionState, UserProfile},
    },
    screens::{self, signin::SignInScreen},
    services::{self, auth::AuthService, org::OrgService, user::UserService},
};

impl ArchiveClient {
    fn fetch_invitations_task(user_email: String) -> Task<Message> {
        Task::perform(
            async move { OrgService::fetch_invitations(&user_email).await },
            |result| Message::Org(OrgMessage::InvitationsLoaded(result)),
        )
    }

    fn re_auth(&mut self) -> Task<Message> {
        self.app.session = SessionState::default();
        self.app.screen = Screen::SignIn(SignInScreen::default());
        Task::none()
    }

    fn run_intent(&self, intent: Intent) -> Task<Message> {
        match intent {
            Intent::FetchInvitations => {
                let email = self.app.session.user.email.clone();
                Self::fetch_invitations_task(email)
            }
            Intent::CreateOrg => Task::perform(
                OrgService::get_or_create_organization(
                    self.app.session.user.access_token.clone(),
                    self.app.session.user.email.clone(),
                ),
                |organisation| Message::Org(OrgMessage::OrgCreated(organisation)),
            ),
        }
    }

    fn handle_error(&mut self, error: GlobalError) -> Task<Message> {
        match error {
            GlobalError::Common(app::message::CommonServiceError::TokenExpired) => {
                if !self.app.is_signed_in() {
                    self.re_auth()
                } else {
                    let refresh_token = self.app.session.user.refresh_token.clone();

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
            _ => Task::none(),
        }
    }

    fn handle_auth_messages(&mut self, message: app::message::AuthMessage) -> Task<Message> {
        match message {
            app::message::AuthMessage::AccessTokenRefreshed(Err(_)) | app::message::AuthMessage::SignedOut => {
                self.re_auth()
            }
            app::message::AuthMessage::AccessTokenRefreshed(Ok(refreshed_token)) => {
                self.app.session.user.access_token = refreshed_token.access_token;
                self.app.session.user.token_type = refreshed_token.token_type;
                self.app.session.user.refresh_token = refreshed_token.refresh_token;
                self.app.session.user.expires_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + refreshed_token.expires_in;

                UserService::save_user_profile(&self.app.session.user);

                if let Some(intent) = self.app.retry_intent {
                    self.run_intent(intent)
                }else {
                    Task::none()
                }
            }
            app::message::AuthMessage::AccessTokenReceived(Err(auth_error)) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.error = Some(auth_error.into());
                }
                Task::none()
            }
            app::message::AuthMessage::AccessTokenReceived(Ok(access_token)) => {
                self.app.session.auth = app::state::AuthState::SignedIn;
                let email = services::auth::AuthService::extract_email_from_access_token(
                    &access_token.id_token,
                );

                let user_email = email.clone();
                let user_profile = UserProfile {
                    email,
                    scopes: access_token
                        .scope
                        .split(" ")
                        .map(|s| s.to_string())
                        .collect(),
                    refresh_token: access_token.refresh_token,
                    expires_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        + access_token.expires_in,
                    token_type: access_token.token_type,
                    access_token: access_token.access_token,
                };

                UserService::save_user_profile(&user_profile);

                self.app.session.user = user_profile;
                self.app.session.auth = app::state::AuthState::SignedIn;

                // Navigate to organization selection screen
                self.app.screen =
                    Screen::OrgSelection(screens::org_selection::OrgSelectionScreen::new());
                self.app.retry_intent = Some(app::state::Intent::FetchInvitations);

                // Fetch invitations for the user
                Self::fetch_invitations_task(user_email)
            }
        }
    }

    // changes the state, switch screen if needed
    pub fn update(&mut self, message: Message) -> Task<Message> {
        println!("{message:?}");

        match message {
            Message::Screen(ScreenMessage::Login(
                msg @ screens::signin::Message::SignInClicked,
            )) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.update(msg.clone());
                    Task::perform(AuthService::get_drive_access_token(), |access_token| {
                        Message::Auth(app::message::AuthMessage::AccessTokenReceived(access_token))
                    })
                } else {
                    Task::none()
                }
            }
            Message::Screen(ScreenMessage::Login(msg @ screens::signin::Message::ClearError)) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.update(msg);
                }

                Task::none()
            }
            Message::Auth(auth_msg) => {
                self.handle_auth_messages(auth_msg)
            }
            Message::Org(OrgMessage::InvitationsLoaded(Ok(invitations))) => {
                if let Screen::OrgSelection(screen) = &mut self.app.screen {
                    screen.invitations = invitations.clone();
                    screen.loading = false;
                }
                Task::none()
            }
            Message::Org(OrgMessage::InvitationsLoaded(Err(err))) => {
                if let Screen::OrgSelection(screen) = &mut self.app.screen {
                    screen.loading = false;
                }
                Task::none()
            }
            Message::Screen(ScreenMessage::OrgSelection(
                msg @ screens::org_selection::Message::CreateOrgClicked,
            )) => {
                if let Screen::OrgSelection(screen) = &mut self.app.screen {
                    screen.update(msg.clone());
                    if self.app.is_signed_in() {
                        Task::perform(
                            OrgService::get_or_create_organization(
                                self.app.session.user.access_token.clone(),
                                self.app.session.user.email.clone(),
                            ),
                            |organisation| Message::Org(OrgMessage::OrgCreated(organisation)),
                        )
                    } else {
                        Task::done(Message::Screen(app::message::ScreenMessage::Login(
                            screens::signin::Message::SignInClicked,
                        )))
                    }
                } else {
                    Task::none()
                }
            }
            Message::Screen(ScreenMessage::OrgSelection(
                screens::org_selection::Message::JoinOrgClicked(org_id),
            )) => {
                // TODO: Join the organization with the given org_id
                println!("Join organization clicked: {}", org_id);
                Task::none()
            }
            _ => {
                println!("Unhandled message");
                Task::none()
            }
        }
    }
}
