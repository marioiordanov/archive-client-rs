use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use iced::{Task, futures::stream::unfold};
use reqwest::header;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use url::Url;

use crate::{
    HTTP,
    app::message::{CommonServiceError, Message, SyncError, SyncMessage},
    constants::{FILES_UPLOAD_URL, FILES_URL},
    services::http::HttpService,
};

pub struct DriveService;

#[derive(Debug, Deserialize)]
struct FileListResponse {
    files: Vec<DriveFileWithParent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DriveFileWithParent {
    #[serde(flatten)]
    pub(crate) file: DriveFile,
    #[serde(
        rename = "parents",
        deserialize_with = "deserialize_parents_into_parent"
    )]
    parent: String,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct DriveRevision {
    pub id: String,
    #[serde(rename = "modifiedTime")]
    pub modified_time: String,
    pub size: Option<String>,
    #[serde(rename = "originalFilename")]
    pub original_filename: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RevisionListResponse {
    revisions: Vec<DriveRevision>,
}

#[derive(Debug, Serialize)]
struct CreateFileMetadata<'a> {
    name: &'a str,
    parents: Vec<&'a str>,
}

#[derive(Debug, Serialize)]
struct CreateFolderMetadata<'a> {
    name: &'a str,
    parents: Vec<&'a str>,
    #[serde(rename = "mimeType")]
    mime_type: &'a str,
}

// V3 GoogleDrive API doesnt support multiple parents
fn deserialize_parents_into_parent<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let parents: Vec<String> = Vec::deserialize(deserializer)?;
    // its not possible for a file to not have parent
    parents
        .into_iter()
        .next()
        .ok_or(serde::de::Error::custom("Unable to decode parent"))
}

#[derive(Deserialize, Debug, Serialize)]
pub(crate) struct FolderResponse {
    id: String,
    name: String,
    #[serde(
        rename = "parents",
        deserialize_with = "deserialize_parents_into_parent"
    )]
    parent: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct FolderResponseList {
    files: Vec<FolderResponse>,
}

impl DriveService {
    pub async fn get_all_objects_bfs(
        root_id: &str,
        access_token: &str,
    ) -> Result<Vec<DriveFileWithParent>, CommonServiceError> {
        let mut all_objects = vec![];
        let mut current_parents = vec![root_id.to_string()];

        while !current_parents.is_empty() {
            let parent_clauses: Vec<String> = current_parents
                .iter()
                .map(|id| format!("'{}' in parents", id))
                .collect();
            let parent_query = parent_clauses.join(" or ");

            let query = format!(
                "q=({}) and trashed=false&fields=files(id,name,parents,mimeType)",
                parent_query
            );

            let resp = HttpService::<()>::new(FILES_URL)
                .auth(access_token)
                .query(&query)
                .get::<FileListResponse, CommonServiceError>()
                .await?;

            current_parents = resp
                .files
                .iter()
                .filter_map(|f| {
                    if let Some(val) = f.file.mime_type.as_ref()
                        && val.eq("application/vnd.google-apps.folder")
                    {
                        return Some(val);
                    }
                    None
                })
                .cloned()
                .collect();

            all_objects.extend(resp.files);
        }

        Ok(all_objects)
    }
    pub async fn get_all_subfolders_bfs(
        root_id: &str,
        access_token: &str,
    ) -> Result<Vec<FolderResponse>, CommonServiceError> {
        let mut all_folders = vec![];
        let mut current_parents = vec![root_id.to_string()];

        while !current_parents.is_empty() {
            let parent_clauses: Vec<String> = current_parents
                .iter()
                .map(|id| format!("'{}' in parents", id))
                .collect();
            let parent_query = parent_clauses.join(" or ");

            let query = format!(
                "q=({}) and trashed=false and mimeType='application/vnd.google-apps.folder'&fields=files(id,name,parents)",
                parent_query
            );

            let resp = HttpService::<()>::new(FILES_URL)
                .auth(access_token)
                .query(&query)
                .get::<FolderResponseList, CommonServiceError>()
                .await?;

            current_parents = resp.files.iter().map(|f| f.id.clone()).collect();
            all_folders.extend(resp.files);
        }

        Ok(all_folders)
    }

    pub async fn delete_object(
        object_id: String,
        access_token: String,
    ) -> Result<(), CommonServiceError> {
        HttpService::<()>::new(&format!("{}/{}", FILES_URL, object_id))
            .auth(access_token)
            .delete_no_response::<CommonServiceError>()
            .await
    }

    pub async fn create_folder(
        parent_id: &str,
        folder_name: &str,
        access_token: &str,
    ) -> Result<DriveFile, CommonServiceError> {
        let metadata = CreateFolderMetadata {
            name: folder_name,
            parents: vec![parent_id],
            mime_type: "application/vnd.google-apps.folder",
        };

        let created = HttpService::new(FILES_URL)
            .auth(access_token)
            .json_body(metadata)
            .post::<DriveFile, CommonServiceError>()
            .await?;

        Ok(created)
    }

    /// from: "f/sub/deep/deepest.txt"), to: path_buf("f/dd.txt"
    pub(crate) async fn move_object(
        object_drive_id: String,
        old_parent_id: String,
        new_parent_id: String,
        access_token: String,
        file_name: String,
    ) -> Result<DriveFile, CommonServiceError> {
        #[derive(Serialize)]
        struct MoveFileRequest {
            name: String,
        }

        let url = format!("{FILES_URL}/{object_drive_id}");
        HttpService::<MoveFileRequest>::new(&url)
            .auth(access_token)
            .json_body(MoveFileRequest { name: file_name })
            .query(&format!(
                "removeParents={old_parent_id}&addParents={new_parent_id}"
            ))
            .patch::<DriveFile, CommonServiceError>()
            .await
    }

    pub async fn find_by_name(
        parent_id: &str,
        file_name: &str,
        access_token: &str,
        is_folder: bool,
    ) -> Result<Option<DriveFileWithParent>, CommonServiceError> {
        let mime_type_operator = if is_folder { "=" } else { "!=" };
        let query = format!(
            "q='{}' in parents and trashed=false and mimeType{}'application/vnd.google-apps.folder' and name='{}'&fields=files(id,name,mimeType,parents)",
            parent_id,
            mime_type_operator,
            escape_drive_query_string(file_name)
        );

        let resp = HttpService::<()>::new(FILES_URL)
            .auth(access_token)
            .query(&query)
            .get::<FileListResponse, CommonServiceError>()
            .await?;

        Ok(resp.files.into_iter().next())
    }

    pub async fn upload_existing_file(
        local_file_path: PathBuf,
        file_id: String,
        access_token: String,
    ) -> Result<DriveFile, SyncError> {
        let bytes = std::fs::read(local_file_path).map_err(|e| SyncError::Io(format!("{}", e)))?;

        let location = Self::start_resumable_update(&file_id, &access_token)
            .await
            .map_err(SyncError::from)?;
        let updated = Self::put_resumable_bytes(location, bytes, &access_token)
            .await
            .map_err(SyncError::from)?;
        Ok(updated)
    }

    pub async fn upload_new_file(
        local_file_path: PathBuf,
        parent_folder_id: String,
        access_token: String,
    ) -> Result<DriveFile, SyncError> {
        let file_name = local_file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(&local_file_path)
            .map_err(|e| SyncError::Io(format!("{} {}", local_file_path.display(), e)))?;

        let location = Self::start_resumable_create(&file_name, &parent_folder_id, &access_token)
            .await
            .map_err(SyncError::from)?;

        let created = Self::put_resumable_bytes(location, bytes, &access_token)
            .await
            .map_err(SyncError::from)?;
        Ok(created)
    }

    pub async fn list_revisions(
        file_id: &str,
        access_token: &str,
    ) -> Result<Vec<DriveRevision>, CommonServiceError> {
        let url = format!("{FILES_URL}/{file_id}/revisions");
        let resp = HttpService::<()>::new(&url)
            .auth(access_token)
            .query("fields=revisions(id,modifiedTime,size,originalFilename)")
            .get::<RevisionListResponse, CommonServiceError>()
            .await?;

        Ok(resp.revisions)
    }

    pub async fn download_revision(
        file_id: &str,
        revision_id: &str,
        access_token: &str,
    ) -> Result<Vec<u8>, CommonServiceError> {
        let mut url =
            Url::from_str(&format!("{FILES_URL}/{file_id}/revisions/{revision_id}")).unwrap();

        HTTP.patch(url.clone())
        .bearer_auth(access_token)
        .json(&json!(
            {
                "keepForever": true
            }
        ))
        .send()
        .await
        .map_err(|e| {
                CommonServiceError::from((e, access_token.to_string()))
            })?;

        url.set_query(Some("alt=media"));

        let bytes = HTTP
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .bytes()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?;

        Ok(bytes.to_vec())
    }

    async fn start_resumable_create(
        file_name: &str,
        parent_id: &str,
        access_token: &str,
    ) -> Result<Url, CommonServiceError> {
        let mut url = Url::from_str(FILES_UPLOAD_URL).unwrap();
        url.set_query(Some("uploadType=resumable"));

        let metadata = CreateFileMetadata {
            name: file_name,
            parents: vec![parent_id],
        };

        let response = HTTP
            .post(url)
            .bearer_auth(access_token)
            .header(header::CONTENT_TYPE, "application/json; charset=UTF-8")
            .header(
                header::HeaderName::from_static("x-upload-content-type"),
                "application/octet-stream",
            )
            .json(&metadata)
            .send()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?;

        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| CommonServiceError::InvalidResponse {
                reason: "Missing resumable upload Location header".to_string(),
            })?;

        Url::from_str(location).map_err(|e| CommonServiceError::InvalidResponse {
            reason: e.to_string(),
        })
    }

    async fn start_resumable_update(
        file_id: &str,
        access_token: &str,
    ) -> Result<Url, CommonServiceError> {
        let mut url = Url::from_str(&format!("{FILES_UPLOAD_URL}/{file_id}")).unwrap();
        url.set_query(Some("uploadType=resumable"));

        let response = HTTP
            .patch(url)
            .bearer_auth(access_token)
            .header(header::CONTENT_TYPE, "application/json; charset=UTF-8")
            .header(
                header::HeaderName::from_static("x-upload-content-type"),
                "application/octet-stream",
            )
            .body("{}")
            .send()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?;

        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| CommonServiceError::InvalidResponse {
                reason: "Missing resumable upload Location header".to_string(),
            })?;

        Url::from_str(location).map_err(|e| CommonServiceError::InvalidResponse {
            reason: e.to_string(),
        })
    }

    async fn put_resumable_bytes(
        location: Url,
        bytes: Vec<u8>,
        access_token: &str,
    ) -> Result<DriveFile, CommonServiceError> {
        HTTP.put(location)
            .bearer_auth(access_token)
            .body(bytes)
            .send()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .json::<DriveFile>()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))
    }
}

fn escape_drive_query_string(value: &str) -> String {
    // Drive query strings use single quotes. Escape with backslash.
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

    use crate::{
        app::{message::CommonServiceError, state::UserProfile},
        constants::FILES_URL,
        services::{
            auth::AuthService,
            drive::{DriveService, FileListResponse},
            http::HttpService,
            local_storage::LocalStorageService,
        },
    };

    #[test]
    fn get_all_folders() {
        let root_dir = PathBuf::from("/Users/mario/Projects/archive-client-rs/app-data");
        let path = PathBuf::from("/Users/mario/Projects/archive-client-rs/app-data/aaaa/aaad/df");

        let relative_path = path.strip_prefix(&root_dir).unwrap();
        let mut current = root_dir;
        let components = relative_path.components();
        let mut paths = Vec::with_capacity(components.count());

        for c in relative_path.components().into_iter() {
            match c {
                std::path::Component::Normal(os_str) => {
                    current = current.join(os_str);
                    paths.push(current.clone());
                }
                _ => {}
            }
        }

        println!("{paths:?}");
    }
}
