use iced::{Element, Task};

mod app;
mod constants;
mod screens;
mod services;
mod ui_error;

use crate::{
    app::{
        message::{Message, OrgMessage},
        state::{AppState, SessionState},
    },
    services::{org::OrgService, user::UserService},
};

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

        let (app, task) = if let Some(profile) = UserService::load_user_profile() {
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

            (
                app,
                Task::perform(
                    async move { OrgService::fetch_invitations(email.as_str()).await },
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
