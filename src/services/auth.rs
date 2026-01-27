use std::{collections::HashMap, convert::Infallible, net::SocketAddr, str::FromStr as _};

use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Bytes, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::sync::Mutex;
use url::Url;

const HTML_SUCCESSFUL_SIGN_IN: &[u8] = include_bytes!("../../sign-in-complete.html");

pub struct AuthService {}

#[derive(Deserialize, Debug, Clone)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub expires_in: u16,
    pub token_type: String,
    pub scope: String,
    pub id_token: Option<String>,
    pub refresh_token: String,
    pub refresh_token_expires_in: Option<u16>,
}

impl AuthService {
    pub async fn get_drive_access_token(&self) -> AccessTokenResponse {
        self.open_browser();
        self.start_local_server_for_single_request().await
    }

    async fn oauth2_redirect_uri_handler(
        &self,
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

    async fn start_local_server_for_single_request(&self) -> AccessTokenResponse {
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
                let access_token = self.oauth2_redirect_uri_handler(req).await.unwrap();
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

    fn open_browser(&self) {
        let mut url = Url::parse(&dotenvy::var("AUTH_URL").unwrap()).unwrap();

        url.query_pairs_mut()
            .append_pair("client_id", &dotenvy::var("CLIENT_ID").unwrap())
            .append_pair("redirect_uri", &dotenvy::var("REDIRECT_URI").unwrap())
            .append_pair("scope", &dotenvy::var("SCOPE").unwrap())
            .append_pair("access_type", "offline")
            .append_pair("response_type", "code");

        webbrowser::open_browser(webbrowser::Browser::Default, url.as_str()).unwrap();
    }
}
