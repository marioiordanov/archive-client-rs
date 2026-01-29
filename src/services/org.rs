use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    app::{
        message::{CommonServiceError, OrgError},
        state::OrgInvitation,
    },
    constants::FILES_URL,
};

#[derive(Deserialize, Debug, Clone)]
pub struct RootFolderEntry {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct RootFolderResponse {
    files: Vec<RootFolderEntry>,
}

pub struct OrgService;

impl OrgService {
    pub async fn fetch_invitations(_user_email: &str) -> Result<Vec<OrgInvitation>, OrgError> {
        // TODO: Make API call to backend to fetch invitations
        // For now, simulate network delay and return mock data

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // MVP: enable mock data when developing UI flows.
        // Disable with: --no-default-features (or remove feature when backend is ready).
        #[cfg(feature = "mock_org")]
        {
            return Ok(vec![
                OrgInvitation {
                    org_id: "1k66jRNSZcyTzLkeTOoKBhpG7amBpffyV".to_string(),
                    org_name: "TESTING".to_string(),
                    invited_by: "mario.iordanov1995@gmail.com".to_string(),
                    invited_at: 1234567890,
                },
                OrgInvitation {
                    org_id: "org_456".to_string(),
                    org_name: "Tech Startup Inc".to_string(),
                    invited_by: "malicious@owner.com".to_string(),
                    invited_at: 1234567891,
                },
            ]);
        }

        #[cfg(not(feature = "mock_org"))]
        {
            // TODO: real backend call
            Err(OrgError::Common(CommonServiceError::TokenExpired))
        }
    }

    pub async fn get_or_create_organization(
        access_token: String,
        owner_email: String,
    ) -> Result<RootFolderEntry, OrgError> {
        if let Ok(id) = OrgService::find_root_folder(&access_token, &owner_email).await {
            return Ok(id);
        }

        #[derive(Serialize)]
        struct Request<'a> {
            name: &'a str,
            #[serde(rename = "mimeType")]
            mime_type: &'a str,
            #[serde(rename = "appProperties")]
            app_properties: HashMap<&'a str, &'a str>,
        }

        let url = Url::parse(FILES_URL).unwrap();

        let mut map = HashMap::new();
        map.insert("archiveClientType", "application");
        map.insert("orgId", &owner_email);

        reqwest::Client::new()
            .post(url)
            .bearer_auth(access_token)
            .json(&Request {
                name: "archive-client-org",
                mime_type: "application/vnd.google-apps.folder",
                app_properties: map,
            })
            .send()
            .await
            .map_err(|e| CommonServiceError::from(e))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from(e))?
            .json::<RootFolderEntry>()
            .await
            .map_err(|e| CommonServiceError::from(e).into())
    }

    async fn find_root_folder(
        access_token: &str,
        owner_email: &str,
    ) -> Result<RootFolderEntry, OrgError> {
        let mut url = Url::parse(FILES_URL).unwrap(); // safe, because it comes from a constant
        let query_string = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("fields", "files(id,name)")
            .append_pair("q", &format!("mimeType='application/vnd.google-apps.folder' and appProperties has {{ key='archiveClientType' and value='application' }} and appProperties has {{ key='orgId' and value='{owner_email}' }} "))
            .finish();

        url.set_query(Some(&query_string));

        let response = reqwest::Client::new()
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| CommonServiceError::from(e))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from(e))?
            .json::<RootFolderResponse>()
            .await
            .map_err(|e| {
                OrgError::from(CommonServiceError::InvalidResponse {
                    reason: e.to_string(),
                })
            })?;

        if response.files.is_empty() {
            Err(OrgError::NoRootFolder)
        } else {
            Ok(response
                .files
                .into_iter()
                .next()
                .expect("Already checked it is non-empty"))
        }
    }
}
