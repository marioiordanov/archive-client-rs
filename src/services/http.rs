use std::{ str::FromStr};

use hyper::header::CONTENT_TYPE;
use reqwest::{ClientBuilder, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::{HTTP, app::message::CommonServiceError};

pub struct HttpService<TRequest: Serialize = ()> {
    url: Url,
    bearer_token: Option<String>,
    form_data: Vec<(String, String)>,
    json_body: Option<TRequest>,
}

impl<TRequest: Serialize> HttpService<TRequest> {
    pub fn new(url_str: &str) -> Self {
        Self {
            url: Url::from_str(url_str).unwrap(),
            bearer_token: None,
            form_data: vec![],
            json_body: None,
        }
    }
    pub fn query(&mut self, new_query: &str) -> &mut Self {
        if let Some(query) = self.url.query() {
            self.url.set_query(Some(&format!("{query}&{new_query}")));
        } else {
            self.url.set_query(Some(new_query));
        }

        self
    }

    pub fn form_data(&mut self, key:impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.form_data.push((key.into(), value.into()));
        self
    }

    pub fn auth(&mut self, bearer_token: impl Into<String>) -> &mut Self {
        self.bearer_token = Some(bearer_token.into());
        self
    }

    fn build_client(self, method: &str) -> RequestBuilder {
        let mut client = match method {
            "get" => HTTP.get(self.url),
            "post" => HTTP.post(self.url),
            "put" => HTTP.put(self.url),
            unimplemented_method => unimplemented!("{unimplemented_method}"),
        };

        if let Some(token) = self.bearer_token {
            client = client.bearer_auth(token);
        }

        client = if let Some(json_request) = self.json_body {
            client.json(&json_request)
        } else {
            let mut body = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in self.form_data.iter() {
                body.append_pair(key, value);
            }

            client = client.header(CONTENT_TYPE, "application/x-www-form-urlencoded");
            client.body(body.finish())
        };

        client
    }

    pub async fn send<TResponse: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<TResponse, CommonServiceError> {
        let request = self.build_client(method);

        request
            .send()
            .await
            .map_err(|e| CommonServiceError::from(e))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from(e))?
            .json::<TResponse>()
            .await
            .map_err(|e| CommonServiceError::from(e))
    }
    pub async fn send_no_response(self, method: &str) -> Result<(), CommonServiceError> {
        let request = self.build_client(method);

        request
            .send()
            .await
            .map_err(|e| CommonServiceError::from(e))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from(e))
            .map(|_| ())
    }
}
