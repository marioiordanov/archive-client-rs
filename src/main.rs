use iced::{Element, Task};
use lazy_static::lazy_static;

mod app;
mod constants;
mod screens;
mod services;
mod ui_error;

use crate::{
    app::{
        message::{Message, OrgMessage},
        state::{AppState, SessionState, UserProfile},
    },
    services::{local_storage::LocalStorageService, org::OrgService},
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
}

impl ArchiveClient {
    fn boot() -> (Self, Task<Message>) {
        println!("Starting Archive Client...");

        let (app, task) = if let Some(profile) = LocalStorageService::load_object::<UserProfile>(
            services::local_storage::ObjectType::UserProfile,
        ) {
            let email = profile.email.clone();
            let app = AppState {
                screen: app::state::Screen::OrgSelection(
                    screens::org_selection::OrgSelectionScreen::new(),
                ),
                session: SessionState {
                    user: profile,
                    role: None,
                    auth: app::state::AuthState::SignedIn,
                },
                org: Default::default(),
                retry_intent: None,
            };

            let access_token = app.session.user.access_token.clone();

            (
                app,
                Task::perform(
                    async move { OrgService::fetch_invitations(email.as_str(), &access_token).await },
                    |result| Message::Org(OrgMessage::InvitationsLoaded(result)),
                ),
            )
        } else {
            let app = AppState {
                screen: app::state::Screen::SignIn(screens::signin::SignInScreen::default()),
                session: Default::default(),
                org: Default::default(),
                retry_intent: None,
            };
            (app, Task::none())
        };

        (Self { app }, task)
    }

    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen {}", self.app.screen);

        let contents = match &self.app.screen {
            app::state::Screen::SignIn(screen) => screen.view().map(|m| Message::Screen(m.into())),
            app::state::Screen::OrgSelection(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
        };

        contents.into()
    }
}
