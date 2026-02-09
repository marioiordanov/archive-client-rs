use std::path::PathBuf;

use iced::{Element, Subscription, Task};
use lazy_static::lazy_static;

mod app;
mod constants;
mod screens;
mod services;
mod ui_error;

use crate::{
    app::{
        message::Message,
        state::{AppState, Intent, OrgState, Role, Screen, SessionState, UserProfile},
    },
    services::local_storage::LocalStorageService,
};

lazy_static! {
    static ref HTTP: reqwest::Client = reqwest::Client::new();
}

fn main() -> iced::Result {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    dotenvy::dotenv().map_err(|e| iced::Error::WindowCreationFailed(e.into()))?;

    iced::application(
        ArchiveClient::boot,
        ArchiveClient::update,
        ArchiveClient::view,
    )
    .centered()
    .subscription(ArchiveClient::subscription)
    .run()
}

struct ArchiveClient {
    app: AppState,
    screen: Screen,
}

impl ArchiveClient {
    fn boot() -> (Self, Task<Message>) {
        println!("Starting Archive Client...");

        let user_profile = LocalStorageService::load_object::<UserProfile>(
            services::local_storage::ObjectType::UserProfile,
        );

        let has_user_profile = user_profile.is_some();

        let org_profile =
            LocalStorageService::load_object::<OrgState>(services::local_storage::ObjectType::Org);

        let has_org_profile = org_profile.is_some();

        let org = if let Some(org) = org_profile {
            org
        } else {
            OrgState::default()
        };

        let mut session = if let Some(user) = user_profile {
            SessionState {
                user,
                auth: app::state::AuthState::SignedIn,
            }
        } else {
            SessionState::default()
        };

        let (state, screen, next_task) = match (has_user_profile, has_org_profile) {
            (true, true) => {
                // Role belongs to the user, not the organization.
                // Backward-compat heuristic if role is missing:
                // - if a local folder is mapped, treat as member/user
                // - otherwise treat as owner
                let role_was_none = session.user.role.is_none();
                let inferred_role = match session.user.role.clone() {
                    Some(r) => Some(r),
                    None if org.config.local_folder_path.is_some() => Some(Role::User),
                    None => Some(Role::Owner),
                };

                session.user.role = inferred_role;

                if role_was_none {
                    LocalStorageService::save_object(
                        &session.user,
                        services::local_storage::ObjectType::UserProfile,
                    );
                }

                match session.user.role.as_ref() {
                    Some(Role::Owner) => {
                        let next_task = ArchiveClient::load_dashboard_task(
                            org.config.archive_folder_id.clone(),
                            session.user.access_token.clone(),
                        );
                        let screen = app::state::Screen::OrgDashboard(
                            screens::org_dashboard::OrgDashboardScreen::new(),
                        );
                        let org_id = org.config.archive_folder_id.clone();
                        let state = AppState {
                            session,
                            org,
                            retry_intent: Some(Intent::LoadDashboard { org_id }),
                        };

                        (state, screen, next_task)
                    }
                    Some(Role::User) => {
                        let mapped = org.config.local_folder_path.clone();
                        let screen = app::state::Screen::OrgSync(
                            screens::org_sync::OrgSyncScreen::new(mapped),
                        );

                        let state = AppState {
                            session,
                            org,
                            retry_intent: None,
                        };

                        (state, screen, Task::none())
                    }
                    None => {
                        // Should be unreachable due to inference above.
                        panic!("Should be unreachable");
                    }
                }
            }
            (true, false) => {
                let screen = app::state::Screen::OrgSelection(
                    screens::org_selection::OrgSelectionScreen::new(),
                );

                let next_task = Self::fetch_invitations_task(
                    session.user.email.clone(),
                    session.user.access_token.clone(),
                );

                let state = AppState {
                    session,
                    org,
                    retry_intent: Some(Intent::FetchInvitations),
                };

                (state, screen, next_task)
            }
            (false, false) => (
                AppState {
                    session,
                    org,
                    retry_intent: None,
                },
                app::state::Screen::SignIn(screens::signin::SignInScreen::default()),
                Task::none(),
            ),
            (false, true) => panic!("Impossible"),
        };

        (Self { app: state, screen }, next_task)
    }

    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen {}", self.screen);

        let contents = match &self.screen {
            app::state::Screen::SignIn(screen) => screen.view().map(|m| Message::Screen(m.into())),
            app::state::Screen::OrgSelection(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::OrgDashboard(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::OrgSync(screen) => screen
                .view(&self.app.org.config.archive_folder_name)
                .map(|m| Message::Screen(m.into())),
        };

        contents.into()
    }

    fn subscription(&self) -> Subscription<Message> {
        match &self.screen {
            Screen::OrgSync(screen) if screen.watching => {
                let Some(mapped) = self.app.org.config.local_folder_path.as_ref() else {
                    return Subscription::none();
                };

                crate::app::subscriptions::fs_watch_subscription(PathBuf::from(mapped))
            }
            _ => Subscription::none(),
        }
    }
}
