use std::{collections::HashMap, str::FromStr};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    app::{message::OrgError, state::OrgInvitation},
    constants::FILES_URL,
    services::http::HttpService,
};

#[derive(Debug, Clone)]
struct InvitedUserFolderEntry {
    folder_id: String,
    email: String,
}

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

#[derive(Debug, Clone)]
pub struct DashboardRowData {
    pub email: String,
    pub folder_id: String,
    pub active: bool,
    pub permission_id: Option<String>,
}

impl OrgService {
    pub async fn load_dashboard(
        organization_id: &str,
        access_token: &str,
    ) -> Result<Vec<DashboardRowData>, OrgError> {
        let user_folders = Self::list_user_folders(organization_id, access_token).await?;

        let mut rows = Vec::with_capacity(user_folders.len());
        for folder in user_folders {
            let active = Self::folder_has_files(&folder.folder_id, access_token).await?;
            let permission_id =
                Self::find_permission_id(&folder.folder_id, &folder.email, access_token).await?;

            rows.push(DashboardRowData {
                email: folder.email,
                folder_id: folder.folder_id,
                active,
                permission_id,
            });
        }

        rows.sort_by(|a, b| a.email.to_lowercase().cmp(&b.email.to_lowercase()));
        Ok(rows)
    }

    pub async fn revoke_user_folder_permission(
        folder_id: &str,
        email: &str,
        permission_id: Option<&str>,
        access_token: &str,
    ) -> Result<(), OrgError> {
        let permission_id = match permission_id {
            Some(id) => Some(id.to_string()),
            None => Self::find_permission_id(folder_id, email, access_token).await?,
        };

        let Some(permission_id) = permission_id else {
            // Nothing to revoke.
            return Ok(());
        };

        let url = format!("{FILES_URL}/{folder_id}/permissions/{permission_id}");
        HttpService::<()>::new(&url)
            .auth(access_token)
            .delete_no_response::<OrgError>()
            .await?;

        Ok(())
    }

    pub async fn invite_user(
        user_email: &str,
        organization_id: &str,
        access_token: &str,
    ) -> Result<(RootFolderEntry, String), OrgError> {
        let mut map: HashMap<&str, &str> = HashMap::new();
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
            .post::<RootFolderEntry, OrgError>()
            .await?;

        let permissions_request = DrivePermissionRequest {
            r#type: "user",
            role: "writer",
            email_address: user_email,
        };

        let mut permissions_url =
            Url::from_str(&format!("{FILES_URL}/{}/permissions", created_file.id)).unwrap();
        permissions_url.set_query(Some("fields=id"));
        permissions_url.set_query(Some("sendNotificationEmail=false"));

        let permissions_result = HttpService::new(permissions_url.as_str())
            .auth(access_token)
            .json_body(permissions_request)
            .post::<serde_json::Value, OrgError>()
            .await;

        let permission_id = match permissions_result {
            // if email is invalid, delete the folder
            Err(OrgError::InvalidEmailInvitation) => {
                let _ = HttpService::<()>::new(&format!("{}/{}", FILES_URL, created_file.id))
                    .auth(access_token)
                    .delete_no_response::<OrgError>()
                    .await;

                return Err(OrgError::InvalidEmailInvitation);
            }
            Err(e) => {
                return Err(e);
            }
            Ok(json_value) => json_value["id"].to_string(),
        };

        Ok((created_file, permission_id))
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
        .get::<SharedWithMeFilesResponse, OrgError>()
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
            .post::<RootFolderEntry, OrgError>()
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
        .get::<RootFolderResponse, OrgError>().await?;

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

    async fn list_user_folders(
        organization_id: &str,
        access_token: &str,
    ) -> Result<Vec<InvitedUserFolderEntry>, OrgError> {
        #[derive(Debug, Deserialize)]
        struct DriveFile {
            id: String,
            #[serde(rename = "appProperties")]
            app_properties: Option<HashMap<String, String>>,
        }

        #[derive(Debug, Deserialize)]
        struct DriveFilesResponse {
            files: Vec<DriveFile>,
        }

        let query = format!(
            "q=mimeType='application/vnd.google-apps.folder' and trashed=false and '{organization_id}' in parents and appProperties has {{ key='application' and value='archive-client' }}",
        );

        let files = HttpService::<()>::new(FILES_URL)
            .auth(access_token)
            .query(&query)
            .query("fields=files(id,appProperties)")
            .get::<DriveFilesResponse, OrgError>()
            .await?
            .files;

        let mut result = Vec::with_capacity(files.len());
        for file in files {
            if let Some(email) = file.app_properties.as_ref().and_then(|p| p.get("user")) {
                result.push(InvitedUserFolderEntry {
                    folder_id: file.id,
                    email: email.clone(),
                });
            }
        }

        Ok(result)
    }

    async fn folder_has_files(folder_id: &str, access_token: &str) -> Result<bool, OrgError> {
        #[derive(Deserialize)]
        struct FileId {
            #[serde(rename = "id")]
            _id: String,
        }

        #[derive(Deserialize)]
        struct FilesResponse {
            files: Vec<FileId>,
        }

        let query = format!("q='{folder_id}' in parents and trashed=false");

        let response = HttpService::<()>::new(FILES_URL)
            .auth(access_token)
            .query("pageSize=1")
            .query("fields=files(id)")
            .query(&query)
            .get::<FilesResponse, OrgError>()
            .await?;

        Ok(!response.files.is_empty())
    }

    async fn find_permission_id(
        folder_id: &str,
        email: &str,
        access_token: &str,
    ) -> Result<Option<String>, OrgError> {
        #[derive(Debug, Deserialize)]
        struct Permission {
            id: String,
            #[serde(rename = "emailAddress")]
            email_address: Option<String>,
            #[serde(rename = "type")]
            permission_type: String,
        }

        #[derive(Debug, Deserialize)]
        struct PermissionsResponse {
            permissions: Vec<Permission>,
        }

        let url = format!("{FILES_URL}/{folder_id}/permissions");
        let response = HttpService::<()>::new(&url)
            .auth(access_token)
            .query("fields=permissions(id,emailAddress,type)")
            .get::<PermissionsResponse, OrgError>()
            .await?;

        let found = response.permissions.into_iter().find(|p| {
            p.permission_type == "user"
                && p.email_address
                    .as_ref()
                    .is_some_and(|a| a.eq_ignore_ascii_case(email))
        });

        Ok(found.map(|p| p.id))
    }
}
