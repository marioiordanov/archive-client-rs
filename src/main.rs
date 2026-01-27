use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode,
    body::{Buf, Bytes},
    server::conn::http1::{self, Builder},
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use iced::widget::column;
use iced::{
    Application, Element, Font, Program, Subscription, Task, Theme, color,
    futures::channel::mpsc,
    widget::{
        Column, button, center_y, rich_text, span,
        text::{self, base},
    },
};
use log::info;
use serde::{Deserialize, de::DeserializeOwned};
use std::net::TcpListener;
use std::thread;
use std::{
    collections::HashMap,
    convert::Infallible,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    str::{FromStr, from_utf8},
    thread::JoinHandle,
};
use tokio::sync::Mutex;

mod screens;
use url::Url;
mod app;
mod services;
mod ui_error;

use crate::{
    app::{
        message::{Message, ScreenMessage},
        state::{AppState, OrgState, Screen, SessionState},
    },
    services::auth::AuthService,
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
    // changes the state
    fn update(&mut self, message: Message) -> Task<Message> {
        println!("Received message: {:?}", message);
        match message {
            Message::Screen(ScreenMessage::Login(
                msg @ screens::signin::Message::SignInClicked,
            )) => {
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.update(msg.clone());
                    Task::perform(AuthService {}.get_drive_access_token(), |c| {
                        Message::Auth(app::message::AuthMessage::AccessTokenReceived(Ok(
                            c.access_token
                        )))
                    })
                } else {
                    Task::none()
                }
            }
            _ => Task::none(),
        }
    }

    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen");

        let contents = match &self.app.screen {
            app::state::Screen::SignIn(screen) => {
                println!("{:?}", screen);
                screen.view().map(|m| Message::Screen(m.into()))
            }
            app::state::Screen::GetOrCreateOrganisation => todo!(),
            app::state::Screen::ListFiles => todo!(),
            app::state::Screen::Syncing => todo!(),
        };

        contents.into()
    }
}
