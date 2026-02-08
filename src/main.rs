use iced::{Element, Task};
use lazy_static::lazy_static;

mod app;
mod constants;
mod screens;
mod services;
mod ui_error;

use crate::{
    app::{
        message::Message,
        state::{AppState, Intent, OrgState, Screen, SessionState, UserProfile},
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

        let session = if let Some(user) = user_profile {
            SessionState {
                user,
                role: None,
                auth: app::state::AuthState::SignedIn,
            }
        } else {
            SessionState::default()
        };

        let (state, screen, next_task) = match (has_user_profile, has_org_profile) {
            (true, true) => {
                let next_task = ArchiveClient::load_dashboard_task(
                    org.config.archive_folder_id.clone(),
                    session.user.access_token.clone(),
                );
                let screen = app::state::Screen::OrgDashboard(
                    screens::org_dashboard::OrgDashboardScreen::new(

                    ),
                );
                let org_id = org.config.archive_folder_id.clone();
                let state = AppState {
                    session,
                    org,
                    retry_intent: Some(Intent::LoadDashboard { org_id }),
                };

                (state, screen, next_task)
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
        };

        contents.into()
    }
}
