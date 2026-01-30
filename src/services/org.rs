use std::{collections::HashMap, str::FromStr};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    HTTP,
    app::{
        message::{CommonServiceError, OrgError},
        state::OrgInvitation,
    },
    constants::FILES_URL,
    services::http::HttpService,
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

#[derive(Serialize)]
struct DrivePermissionRequest<'a> {
    r#type: &'a str,
    role: &'a str,
    #[serde(rename = "emailAddress")]
    email_address: &'a str,
}
#[derive(Serialize)]
struct DriveFileRequest<'a> {
    name: &'a str,
    #[serde(rename = "mimeType")]
    mime_type: &'a str,
    #[serde(rename = "appProperties")]
    app_properties: HashMap<&'a str, &'a str>,
    parents: Vec<&'a str>,
}

pub struct OrgService;

impl OrgService {
    pub async fn invite_user(
        user_email: &str,
        organization_id: &str,
        access_token: &str,
    ) -> Result<RootFolderEntry, OrgError> {
        let mut map = HashMap::new();
        map.insert("application", "archive-client");
        map.insert("user", user_email);

        let create_file_request = DriveFileRequest {
            name: user_email,
            mime_type: "application/vnd.google-apps.folder",
            app_properties: map,
            parents: vec![organization_id],
        };

        let created_file = HttpService::new(FILES_URL)
            .auth(access_token)
            .json_body(create_file_request)
            .post::<RootFolderEntry>()
            .await?;

        let permissions_request = DrivePermissionRequest {
            r#type: "user",
            role: "writer",
            email_address: user_email,
        };

        let mut permissions_url =
            Url::from_str(format!("{FILES_URL}/{}/permissions", created_file.id).as_str()).unwrap();
        permissions_url.set_query(Some("fields=id,role"));
        permissions_url.set_query(Some("sendNotificationEmail=false"));

        HttpService::new(permissions_url.as_str())
            .auth(access_token)
            .json_body(permissions_request)
            .post_no_response()
            .await?;

        Ok(created_file)
    }
    pub async fn fetch_invitations(
        user_email: &str,
        access_token: &str,
    ) -> Result<Vec<OrgInvitation>, OrgError> {
        #[derive(Debug, Deserialize)]
        pub struct SharedWithMeFile {
            pub id: String,
            pub name: String,
            #[serde(rename = "sharingUser")]
            pub sharing_user: SharingUser,
        }

        #[derive(Debug, Deserialize)]
        pub struct SharingUser {
            #[serde(rename = "emailAddress")]
            pub email_address: String,
        }

        #[derive(Debug, Deserialize)]
        pub struct SharedWithMeFilesResponse {
            pub files: Vec<SharedWithMeFile>,
        }

        let orgs = HttpService::<()>::new(FILES_URL)
        .auth(access_token)
        .query(&format!("q=sharedWithMe=true and trashed=false and mimeType='application/vnd.google-apps.folder' and appProperties has {{ key='application' and value='archive-client' }} and appProperties has {{ key='user' and value='{user_email}' }} "))
        .query("fields=files(id,name,appProperties,sharingUser(emailAddress),sharedWithMeTime)")
        .get::<SharedWithMeFilesResponse>()
        .await?.files.into_iter().map(|s| OrgInvitation{ org_id: s.id, org_name: s.name, invited_by: s.sharing_user.email_address })
        .collect::<Vec<OrgInvitation>>();

        // MVP: enable mock data when developing UI flows.
        // Disable with: --no-default-features (or remove feature when backend is ready).
        #[cfg(feature = "mock_org")]
        {
            let mut result = vec![
                OrgInvitation {
                    org_id: "1k66jRNSZcyTzLkeTOoKBhpG7amBpffyV".to_string(),
                    org_name: "TESTING".to_string(),
                    invited_by: "hueber9500@gmail.com".to_string(),
                },
                OrgInvitation {
                    org_id: "org_456".to_string(),
                    org_name: "Tech Startup Inc".to_string(),
                    invited_by: "malicious@owner.com".to_string(),
                },
            ];
            result.extend_from_slice(&orgs);
            Ok(result)
        }

        #[cfg(not(feature = "mock_org"))]
        {
            // TODO: real backend call
            Ok(orgs)
        }
    }

    /// Organization is a root folder
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

        HttpService::new(FILES_URL)
            .auth(access_token)
            .json_body(Request {
                name: "archive-client-org",
                mime_type: "application/vnd.google-apps.folder",
                app_properties: map,
            })
            .post::<RootFolderEntry>()
            .await
            .map_err(|e| e.into())
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

        let response = HttpService::<()>::new(FILES_URL)
        .query("fields=files(id,name)")
        .query(&format!("q=mimeType='application/vnd.google-apps.folder' and appProperties has {{ key='archiveClientType' and value='application' }} and appProperties has {{ key='orgId' and value='{owner_email}' }} "))
        .auth(access_token)
        .get::<RootFolderResponse>().await?;

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
