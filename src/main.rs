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
        state::{AppState, Intent, Screen, SessionState, UserProfile},
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

        let (app, screen, task) = if let Some(profile) = LocalStorageService::load_object::<UserProfile>(
            services::local_storage::ObjectType::UserProfile,
        ) {
            let email = profile.email.clone();
            let app = AppState {
                session: SessionState {
                    user: profile,
                    role: None,
                    auth: app::state::AuthState::SignedIn,
                },
                org: Default::default(),
                retry_intent: Some(Intent::FetchInvitations),
            };

            let screen= app::state::Screen::OrgSelection(
                    screens::org_selection::OrgSelectionScreen::new(),
                );

            let access_token = app.session.user.access_token.clone();

            (
                app,
                screen,
                ArchiveClient::fetch_invitations_task(email, access_token),
            )
        } else {
            let app = AppState {
                session: Default::default(),
                org: Default::default(),
                retry_intent: None,
            };
            let screen = app::state::Screen::SignIn(screens::signin::SignInScreen::default());
            (app,screen, Task::none())
        };

        (Self { app, screen }, task)
    }

    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen {}", self.screen);

        let contents = match &self.screen {
            app::state::Screen::SignIn(screen) => screen.view().map(|m| Message::Screen(m.into())),
            app::state::Screen::OrgSelection(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::InviteMembers(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::OrgDashboard(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
        };

        contents.into()
    }
}
