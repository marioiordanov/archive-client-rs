use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::{Message, ScreenMessage},
        state::{Screen, UserProfile},
    },
    screens,
    services::{self, auth::AuthService},
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

                Task::none()
            }
            _ => Task::none(),
        }
    }
}
