use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;

use crate::{
    ArchiveClient, UserState,
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
    let mut user_profile: UserProfile =
        LocalStorageService::load_object(services::local_storage::ObjectType::UserProfile)
            .unwrap_or_default();

    archive_client.app.session.user.access_token = refreshed_token.access_token.clone();
    archive_client.app.session.user.token_type = refreshed_token.token_type.clone();
    archive_client.app.session.user.expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + refreshed_token.expires_in;

    user_profile.access_token = refreshed_token.access_token;
    user_profile.token_type = refreshed_token.token_type;
    user_profile.expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + refreshed_token.expires_in;

    let user_data = match &mut archive_client.app.user_state {
        UserState::SignedIn { user_data } => user_data,
        UserState::OrgCreated { user_data, .. } => user_data,
        UserState::OrgJoined { user_data, .. } => user_data,
        UserState::OrgSynced { user_data, .. } => user_data,
        _ => {
            panic!("Impossible case")
        }
    };
    user_data.access_token = user_profile.access_token.clone();

    LocalStorageService::save_object(
        &user_profile,
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

    // Preserve any previously-known role (e.g. if the user re-auths).
    let role = state.session.user.role.clone();

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
        role,
    };

    LocalStorageService::save_object(
        &user_profile,
        services::local_storage::ObjectType::UserProfile,
    );

    state.user_state.sign_in( user_profile.clone().into());

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
        match (&mut self.app.user_state, &mut self.screen, message) {
            (
                user_state,
                _,
                app::message::AuthMessage::AccessTokenRefreshed(Ok(refreshed_token)),
            ) if !matches!(user_state, UserState::SignedOut) => {
                // when signed out, user cannot refresh its token
                on_access_token_refreshed_ok(self, refreshed_token)
            }

            (
                crate::UserState::SignedOut,
                screen,
                app::message::AuthMessage::AccessTokenReceived(Ok(access_token)),
            ) => on_access_token_received_ok(&mut self.app, screen, access_token),
            (
                crate::UserState::SignedOut,
                Screen::SignIn(screen),
                app::message::AuthMessage::AccessTokenReceived(Err(auth_error)),
            ) => on_access_token_received_err(screen, auth_error),
            (
                user_state,
                _,
                app::message::AuthMessage::AccessTokenRefreshed(Err(_))
                | app::message::AuthMessage::SignedOut,
            ) if !matches!(user_state, UserState::SignedOut) => self.re_auth(), // user_state must be NOT SignedOut
            _ => Task::none(),
        }
    }
}
