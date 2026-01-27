use std::{collections::HashMap, convert::Infallible, net::SocketAddr, str::FromStr as _};

use base64::{Engine, engine::general_purpose};
use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Bytes, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::sync::Mutex;
use url::Url;

const HTML_SUCCESSFUL_SIGN_IN: &[u8] = include_bytes!("../../sign-in-complete.html");

const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/drive.metadata.readonly",
    "email",
];

pub struct AuthService;

#[derive(Deserialize, Debug, Clone)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
    pub scope: String,
    pub refresh_token: String,
    pub id_token: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
    pub refresh_token: String,
}

impl AuthService {
    pub fn extract_email_from_access_token(&self, id_token: &str) -> String {
        let parts: Vec<&str> = id_token.split(".").collect();
        if parts.len() != 3 {
            panic!("Invalid JWT format");
        }

        let payload = parts[1];
        let decoded = general_purpose::STANDARD.decode(payload).unwrap();

        #[derive(Deserialize)]
        struct IdTokenPayload {
            email: String,
        }

        serde_json::from_slice::<IdTokenPayload>(decoded.as_slice())
            .unwrap()
            .email
    }
    pub async fn refresh_access_token(&self, refresh_token: &str) -> RefreshTokenResponse {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", &dotenvy::var("CLIENT_ID").unwrap())
            .append_pair("refresh_token", refresh_token)
            .append_pair("grant_type", "refresh_token")
            .append_pair("client_secret", &dotenvy::var("CLIENT_SECRET").unwrap())
            .finish();

        let response: RefreshTokenResponse = reqwest::Client::new()
            .post(Url::from_str(&dotenvy::var("TOKEN_URL").unwrap()).unwrap())
            .body(body.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        response
    }
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

                return Ok(reqwest::Client::new()
                    .post(Url::from_str(&dotenvy::var("TOKEN_URL").unwrap()).unwrap())
                    .body(body.clone())
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .send()
                    .await
                    .unwrap()
                    .json::<AccessTokenResponse>()
                    .await
                    .unwrap());
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
            .append_pair("scope", &SCOPES.join(" "))
            .append_pair("access_type", "offline")
            .append_pair("response_type", "code");

        webbrowser::open_browser(webbrowser::Browser::Default, url.as_str()).unwrap();
    }
}
