use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::{
    ArchiveClient,
    app::{
        self,
        message::Message,
        state::{AppState, Screen, UserProfile},
    },
    screens::{self, signin::SignInScreen},
    services::{self, local_storage::LocalStorageService},
};

fn on_access_token_refreshed_ok(
    archive_client: &mut ArchiveClient,
    refreshed_token: services::auth::RefreshTokenResponse,
) -> Task<Message> {
    archive_client.app.session.user.access_token = refreshed_token.access_token;
    archive_client.app.session.user.token_type = refreshed_token.token_type;
    archive_client.app.session.user.expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + refreshed_token.expires_in;

    LocalStorageService::save_object(
        &archive_client.app.session.user,
        services::local_storage::ObjectType::UserProfile,
    );

    archive_client.retry_intent()
}

fn on_access_token_received_err(
    screen: &mut SignInScreen,
    auth_error: app::message::AuthError,
) -> Task<Message> {
    screen.error = Some(auth_error.into());
    Task::none()
}

fn on_access_token_received_ok(
    state: &mut AppState,
    screen: &mut Screen,
    access_token: services::auth::AccessTokenResponse,
) -> Task<Message> {
    state.session.auth = app::state::AuthState::SignedIn;
    let email =
        services::auth::AuthService::extract_email_from_access_token(&access_token.id_token);

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

    state.session.user = user_profile;
    state.session.auth = app::state::AuthState::SignedIn;

    // Navigate to organization selection screen
    *screen = Screen::OrgSelection(screens::org_selection::OrgSelectionScreen::new());
    state.retry_intent = Some(app::state::Intent::FetchInvitations);

    // Fetch invitations for the user
    ArchiveClient::fetch_invitations_task(user_email, state.session.user.access_token.clone())
}

impl ArchiveClient {
    pub fn handle_auth_messages(&mut self, message: app::message::AuthMessage) -> Task<Message> {
        match (&mut self.screen, message) {
            (_, app::message::AuthMessage::AccessTokenRefreshed(Ok(refreshed_token))) => {
                on_access_token_refreshed_ok(self, refreshed_token)
            }

            (screen, app::message::AuthMessage::AccessTokenReceived(Ok(access_token))) => {
                on_access_token_received_ok(&mut self.app, screen, access_token)
            }
            (
                Screen::SignIn(screen),
                app::message::AuthMessage::AccessTokenReceived(Err(auth_error)),
            ) => on_access_token_received_err(screen, auth_error),
            (
                _,
                app::message::AuthMessage::AccessTokenRefreshed(Err(_))
                | app::message::AuthMessage::SignedOut,
            ) => self.re_auth(),
            _ => Task::none()
        }
    }
}
