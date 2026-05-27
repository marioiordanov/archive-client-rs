use std::str::FromStr;

use hyper::header::CONTENT_TYPE;
use reqwest::RequestBuilder;
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::HTTP;

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
            "patch" => HTTP.patch(self.url),
            unimplemented_method => unimplemented!("{unimplemented_method}"),
        };

        if let Some(token) = self.bearer_token {
            request = request.bearer_auth(token);
        }

        let has_form_data = !self.form_data.is_empty();

        request = match (self.json_body, has_form_data) {
            (None, false) => request,
            (Some(json_request), false) => request.json(&json_request),
            (None, true) => {
                let mut body = url::form_urlencoded::Serializer::new(String::new());
                for (key, value) in self.form_data.iter() {
                    body.append_pair(key, value);
                }

                request = request.header(CONTENT_TYPE, "application/x-www-form-urlencoded");
                request.body(body.finish())
            }
            _ => {
                unimplemented!()
            }
        };

        request
    }

    #[allow(dead_code)]
    pub async fn put<TResponse: DeserializeOwned, TError: From<(reqwest::Error, String)>>(
        self,
    ) -> Result<TResponse, TError> {
        self.send::<TResponse, TError>("put").await
    }

    pub async fn patch<TResponse: DeserializeOwned, TError: From<(reqwest::Error, String)>>(
        self,
    ) -> Result<TResponse, TError> {
        self.send::<TResponse, TError>("patch").await
    }

    pub async fn post<TResponse: DeserializeOwned, TError: From<(reqwest::Error, String)>>(
        self,
    ) -> Result<TResponse, TError> {
        self.send::<TResponse, TError>("post").await
    }

    #[allow(dead_code)]
    pub async fn post_no_response<TError: From<(reqwest::Error, String)>>(
        self,
    ) -> Result<(), TError> {
        self.send_no_response("post").await
    }

    pub async fn get<TResponse: DeserializeOwned, TError: From<(reqwest::Error, String)>>(
        self,
    ) -> Result<TResponse, TError> {
        self.send::<TResponse, TError>("get").await
    }

    pub async fn delete_no_response<TError: From<(reqwest::Error, String)>>(
        self,
    ) -> Result<(), TError> {
        self.send_no_response::<TError>("delete").await
    }

    async fn send<TResponse: DeserializeOwned, TError: From<(reqwest::Error, String)>>(
        self,
        method: &str,
    ) -> Result<TResponse, TError> {
        let access_token = self.bearer_token.clone().unwrap_or_default();
        let request = self.build_request(method);

        request
            .send()
            .await
            .map_err(|e| TError::from((e, access_token.clone())))?
            .error_for_status()
            .map_err(|e| TError::from((e, access_token.clone())))?
            .json::<TResponse>()
            .await
            .map_err(|e| TError::from((e, access_token)))
    }
    async fn send_no_response<TError: From<(reqwest::Error, String)>>(
        self,
        method: &str,
    ) -> Result<(), TError> {
        let access_token = self.bearer_token.clone().unwrap_or_default();
        let request = self.build_request(method);

        request
            .send()
            .await
            .map_err(|e| TError::from((e, access_token.clone())))?
            .error_for_status()
            .map_err(|e| TError::from((e, access_token)))
            .map(|_| ())
    }
}
