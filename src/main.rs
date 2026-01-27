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
use tokio::{ sync::Mutex};

mod screens;
use url::Url;
mod app;
mod ui_error;
mod services;

use crate::app::{message::{Message, ScreenMessage}, state::{AppState, OrgState, Screen, SessionState}};


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
    app: AppState
}

impl Default for ArchiveClient {
    fn default() -> Self {
        println!("Starting Archive Client...");
        let app_state = AppState {
            screen: app::state::Screen::SignIn(screens::signin::SignInScreen::default()),
            session: SessionState{
                user: None,
                role: None,
                auth: app::state::AuthState::SignedOut,
            },
            org: OrgState {
                config: None,
                status: app::state::OrgStatus::Unknown,
            },
        };
        Self {
            app: app_state
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct AccessTokenResponse {
    access_token: String,
    expires_in: u16,
    token_type: String,
    scope: String,
    id_token: Option<String>,
    refresh_token: String,
    refresh_token_expires_in: Option<u16>,
}

async fn oauth2_redirect_uri_handler(
    req: Request<hyper::body::Incoming>,
) -> Result<AccessTokenResponse, String> {
    if let Some(query) = req.uri().query() {
        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        if let Some(code) = params.get("code") {
            let body = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("code", &code)
                .append_pair("client_id", &dotenvy::var("CLIENT_ID").unwrap())
                .append_pair("redirect_uri", &dotenvy::var("REDIRECT_URI").unwrap())
                .append_pair("grant_type", "authorization_code")
                .append_pair("client_secret", &dotenvy::var("CLIENT_SECRET").unwrap())
                .finish();

            let response: AccessTokenResponse = reqwest::Client::new()
                .post(Url::from_str(&dotenvy::var("TOKEN_URL").unwrap()).unwrap())
                .body(body.clone())
                .header("Content-Type", "application/x-www-form-urlencoded")
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

            return Ok(response);
        }
    }

    Err("Unable to get access token".to_string())
}

impl ArchiveClient {
    async fn start_local_server_for_single_request() -> AccessTokenResponse {
        let redirect_uri = dotenvy::var("REDIRECT_URI").unwrap();
        let uri = Url::parse(&redirect_uri).unwrap();
        let socket: SocketAddr = format!("{}:{}", uri.host_str().unwrap(), uri.port().unwrap())
            .parse()
            .unwrap();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Arc::new(Mutex::new(Some(tx)));

        let listener = tokio::net::TcpListener::bind(socket).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let service = service_fn(move |req| {
            let sender = tx.clone();
            async move {
                let access_token = oauth2_redirect_uri_handler(req).await.unwrap();
                if let Some(sender) = sender.lock().await.take() {
                    sender.send(access_token).unwrap();
                }

                Ok::<Response<Full<Bytes>>, Infallible>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/html")
                        .body(Full::new(Bytes::from_static(HTML_SUCCESSFUL_SIGN_IN)))
                        .unwrap(),
                )
            }
        });

        let _ = http1::Builder::new()
            .keep_alive(false)
            .serve_connection(io, service)
            .await;

        rx.await.unwrap()
    }

    fn open_browser() {
        let mut url = Url::parse(&dotenvy::var("AUTH_URL").unwrap()).unwrap();

        url.query_pairs_mut()
            .append_pair("client_id", &dotenvy::var("CLIENT_ID").unwrap())
            .append_pair("redirect_uri", &dotenvy::var("REDIRECT_URI").unwrap())
            .append_pair("scope", &dotenvy::var("SCOPE").unwrap())
            .append_pair("access_type", "offline")
            .append_pair("response_type", "code");

        webbrowser::open_browser(webbrowser::Browser::Default, url.as_str()).unwrap();
    }

    // changes the state
    fn update(&mut self, message: Message) -> Task<Message> {
        println!("Received message: {:?}", message);
        match message {
            Message::Screen(ScreenMessage::Login(msg @ screens::signin::Message::SignInClicked)) =>
                if let Screen::SignIn(screen) = &mut self.app.screen {
                    screen.update(msg.clone());
                    Task::perform(async {
                    ArchiveClient::open_browser();
                    ArchiveClient::start_local_server_for_single_request().await
                }, |c| Message::Auth(app::message::AuthMessage::AccessTokenReceived(Ok(c.access_token))))
                }else {
                    Task::none()
                }
            ,
            _ => Task::none()
        }
    }

    // update the UI
    fn view(&self) -> Element<'_, Message> {
        println!("Rendering view for screen");

        let contents = match &self.app.screen {
            app::state::Screen::SignIn(screen) => {
                println!("{:?}", screen);
                screen.view()
                .map(|m| Message::Screen(m.into()))
            },
            app::state::Screen::GetOrCreateOrganisation => todo!(),
            app::state::Screen::ListFiles => todo!(),
            app::state::Screen::Syncing => todo!(),
        };

        contents.into()
    }
}
