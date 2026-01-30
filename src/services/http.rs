use std::{any::type_name, str::FromStr};

use reqwest::{ClientBuilder, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::{HTTP, app::message::CommonServiceError};

struct HttpService<TRequest: Serialize> {
    url: Url,
    bearer_token: Option<String>,
    form_data: Vec<(String, String)>,
    json_body: Option<TRequest>,
}

impl<TRequest: Serialize> HttpService<TRequest> {
    fn new(url_str: &str) -> Self {
        Self {
            url: Url::from_str(url_str).unwrap(),
            bearer_token: None,
            form_data: vec![],
            json_body: None,
        }
    }
    fn query(&mut self, new_query: &str) -> &mut Self {
        if let Some(query) = self.url.query() {
            self.url.set_query(Some(&format!("{query}&{new_query}")));
        } else {
            self.url.set_query(Some(new_query));
        }

        self
    }

    fn auth(&mut self, bearer_token: String) -> &mut Self {
        self.bearer_token = Some(bearer_token);
        self
    }

    fn build_client(self, method: &str) -> RequestBuilder {
        let mut client = match method {
            "get" => HTTP.get(self.url),
            "post" => HTTP.post(self.url),
            "put" => HTTP.put(self.url),
            _ => unimplemented!("HTTP METHOD"),
        };

        if let Some(token) = self.bearer_token {
            client = client.bearer_auth(token);
        }

        let content_type = if self.json_body.is_some() {
            "application/json"
        } else {
            "application/x-www-form-urlencoded"
        };

        client = client.header("Content-Type", content_type);

        client = if let Some(json_request) = self.json_body {
            client.json(&json_request)
        } else {
            let mut body = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in self.form_data.iter() {
                body.append_pair(key, value);
            }

            client.body(body.finish())
        };

        client
    }

    async fn send<TResponse: DeserializeOwned>(
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
    async fn send_no_response(self, method: &str) -> Result<(), CommonServiceError> {
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

#[cfg(test)]
mod tests {
    use std::any::type_name;

    use serde::{Serialize, de::DeserializeOwned};

    use crate::{constants::FILES_URL, services::http::HttpService};

    fn de<T: DeserializeOwned>(json_str: &str) -> T {
        serde_json::from_str::<T>(json_str).unwrap()
    }

    fn check_type<T: DeserializeOwned>() {
        println!("{}", type_name::<T>());
    }
    #[test]
    fn x() {
        check_type::<()>();
    }
}
