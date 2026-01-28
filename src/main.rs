use iced::Element;

mod app;
mod constants;
mod screens;
mod services;
mod ui_error;

use crate::app::{
    message::Message,
    state::{AppState, OrgState, SessionState},
};

fn main() -> iced::Result {
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    dotenvy::dotenv().map_err(|e| iced::Error::WindowCreationFailed(e.into()))?;

    iced::application(
        ArchiveClient::default,
        ArchiveClient::update,
        ArchiveClient::view,
    )
    .centered()
    .run()
}

struct ArchiveClient {
    app: AppState,
}

impl Default for ArchiveClient {
    fn default() -> Self {
        println!("Starting Archive Client...");
        let app_state = AppState {
            screen: app::state::Screen::SignIn(screens::signin::SignInScreen::default()),
            session: SessionState {
                user: None,
                role: None,
                auth: app::state::AuthState::SignedOut,
            },
            org: OrgState {
                config: None,
                status: app::state::OrgStatus::Unknown,
            },
        };
        Self { app: app_state }
    }
}

impl ArchiveClient {
    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen");

        let contents = match &self.app.screen {
            app::state::Screen::SignIn(screen) => {
                println!("{:?}", screen);
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::OrgSelection(screen) => {
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::ListFiles => todo!(),
            app::state::Screen::Syncing => todo!(),
        };

        contents.into()
    }
}
