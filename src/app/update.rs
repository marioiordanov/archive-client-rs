use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, OrgMessage, ScreenMessage},
        state::{Screen, UserProfile},
    },
    screens,
    services::{self, auth::AuthService, org::OrgService},
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
                    Task::perform(AuthService {}.get_drive_access_token(), |access_token| {
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
                let email = services::auth::AuthService {}
                    .extract_email_from_access_token(&access_token.id_token);

                let user_email = email.clone();

                self.app.session.user = Some(UserProfile {
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
                });
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
                    self.app.org.invitations = invitations;
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
                screens::org_selection::Message::CreateOrgClicked,
            )) => {
                // TODO: Navigate to create org screen or trigger create org flow
                println!("Create organization clicked");
                Task::none()
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
