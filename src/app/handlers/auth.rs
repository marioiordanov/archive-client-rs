use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::Message,
        state::{Screen, UserProfile},
    },
    screens,
    services::{self, local_storage::LocalStorageService},
};

impl ArchiveClient {
    pub fn handle_auth_messages(&mut self, message: app::message::AuthMessage) -> Task<Message> {
        match message {
            app::message::AuthMessage::AccessTokenRefreshed(Err(_))
            | app::message::AuthMessage::SignedOut => self.re_auth(),
            app::message::AuthMessage::AccessTokenRefreshed(Ok(refreshed_token)) => {
                self.app.session.user.access_token = refreshed_token.access_token;
                self.app.session.user.token_type = refreshed_token.token_type;
                self.app.session.user.expires_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + refreshed_token.expires_in;

                LocalStorageService::save_object(
                    &self.app.session.user,
                    services::local_storage::ObjectType::UserProfile,
                );

                if let Some(intent) = &self.app.retry_intent {
                    self.run_intent(intent)
                } else {
                    Task::none()
                }
            }
            app::message::AuthMessage::AccessTokenReceived(Err(auth_error)) => {
                if let Screen::SignIn(screen) = &mut self.screen {
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

                LocalStorageService::save_object(
                    &user_profile,
                    services::local_storage::ObjectType::UserProfile,
                );

                self.app.session.user = user_profile;
                self.app.session.auth = app::state::AuthState::SignedIn;

                // Navigate to organization selection screen
                self.screen =
                    Screen::OrgSelection(screens::org_selection::OrgSelectionScreen::new());
                self.app.retry_intent = Some(app::state::Intent::FetchInvitations);

                // Fetch invitations for the user
                Self::fetch_invitations_task(user_email, self.app.session.user.access_token.clone())
            }
        }
    }
}
