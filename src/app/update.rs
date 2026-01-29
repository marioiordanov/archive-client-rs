use std::time::{SystemTime, UNIX_EPOCH};

use iced::{Task, futures::FutureExt};

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgMessage, ScreenMessage},
        state::{Screen, UserProfile},
    },
    screens::{self, signin::SignInScreen},
    services::{self, auth::AuthService, org::OrgService, user::UserService},
};

impl ArchiveClient {
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
            Message::Auth(app::message::AuthMessage::AccessTokenReceived(Err(auth_error))) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.error = Some(auth_error.into());
                }
                Task::none()
            }
            Message::Auth(app::message::AuthMessage::AccessTokenReceived(Ok(access_token))) => {
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

                self.app.session.user = Some(user_profile);
                self.app.session.auth = app::state::AuthState::SignedIn;

                // Navigate to organization selection screen
                self.app.screen =
                    Screen::OrgSelection(screens::org_selection::OrgSelectionScreen::new());

                // Fetch invitations for the user
                Task::perform(
                    async move { OrgService::fetch_invitations(&user_email).await },
                    |result| Message::Org(OrgMessage::InvitationsLoaded(result)),
                )
            }
            Message::Org(OrgMessage::InvitationsLoaded(Ok(invitations))) => {
                if let Screen::OrgSelection(screen) = &mut self.app.screen {
                    screen.invitations = invitations.clone();
                    screen.loading = false;
                }
                Task::none()
            }
            Message::Org(OrgMessage::InvitationsLoaded(Err(_err))) => {
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
                    if let Some(user) = self.app.session.user.as_ref() {
                        Task::perform(
                            OrgService::get_or_create_organization(
                                user.access_token.clone(),
                                user.email.clone(),
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
            _ => Task::none(),
        }
    }
}
