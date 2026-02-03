use std::str::FromStr;

use hyper::header::CONTENT_TYPE;
use reqwest::RequestBuilder;
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::{HTTP, app::message::CommonServiceError};

pub struct HttpService<TRequest: Serialize> {
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
    pub fn query(mut self, new_query: &str) -> Self {
        if let Some(query) = self.url.query() {
            self.url.set_query(Some(&format!("{query}&{new_query}")));
        } else {
            self.url.set_query(Some(new_query));
        }

        self
    }

    pub fn form_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.form_data.push((key.into(), value.into()));
        self
    }

    pub fn auth(mut self, bearer_token: impl Into<String>) -> Self {
        self.bearer_token = Some(bearer_token.into());
        self
    }

    pub fn json_body(mut self, json_obj: TRequest) -> Self {
        self.json_body = Some(json_obj);
        self
    }

    fn build_request(self, method: &str) -> RequestBuilder {
        let mut request = match method {
            "get" => HTTP.get(self.url),
            "post" => HTTP.post(self.url),
            "put" => HTTP.put(self.url),
            "delete" => HTTP.delete(self.url),
            unimplemented_method => unimplemented!("{unimplemented_method}"),
        };

        if let Some(token) = self.bearer_token {
            request = request.bearer_auth(token);
        }

        request = if let Some(json_request) = self.json_body {
            request.json(&json_request)
        } else {
            let mut body = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in self.form_data.iter() {
                body.append_pair(key, value);
            }

            request = request.header(CONTENT_TYPE, "application/x-www-form-urlencoded");
            request.body(body.finish())
        };

        request
    }

    pub async fn post<TResponse: DeserializeOwned>(self) -> Result<TResponse, CommonServiceError> {
        self.send::<TResponse>("post").await
    }

    pub async fn post_no_response(self) -> Result<(), CommonServiceError> {
        self.send_no_response("post").await
    }

    pub async fn get<TResponse: DeserializeOwned>(self) -> Result<TResponse, CommonServiceError> {
        self.send::<TResponse>("get").await
    }

    pub async fn delete_no_response(self) -> Result<(), CommonServiceError> {
        self.send_no_response("delete").await
    }

    async fn send<TResponse: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<TResponse, CommonServiceError> {
        let request = self.build_request(method);

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
    async fn send_no_response(self, method: &str) -> Result<(), CommonServiceError> {
        let request = self.build_request(method);

        request
            .send()
            .await
            .map_err(|e| CommonServiceError::from(e))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from(e))
            .map(|_| ())
    }
}
