use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use iced::{Task, futures::stream::unfold};
use reqwest::header;
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;
use warp::filters::ext::get;

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

#[derive(Debug, Deserialize, Clone)]
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
                    if let Some(val) = f.file.mime_type.as_ref() {
                        if val.eq("application/vnd.google-apps.folder") {
                            return Some(val);
                        }
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

    pub async fn find_child_folder(
        parent_id: &str,
        folder_name: &str,
        access_token: &str,
    ) -> Result<Option<DriveFileWithParent>, CommonServiceError> {
        let query = format!(
            "q='{}' in parents and trashed=false and mimeType='application/vnd.google-apps.folder' and name='{}'&fields=files(id,name,mimeType)",
            parent_id,
            escape_drive_query_string(folder_name)
        );

        let resp = HttpService::<()>::new(FILES_URL)
            .auth(access_token)
            .query(&query)
            .get::<FileListResponse, CommonServiceError>()
            .await?;

        Ok(resp.files.into_iter().next())
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

    pub async fn ensure_remote_folder_path(
        root_folder_id: &str,
        path_segments: &[String],
        access_token: &str,
    ) -> Result<String, CommonServiceError> {
        let mut current = root_folder_id.to_string();
        for seg in path_segments {
            let found = Self::find_child_folder(&current, seg, access_token).await?;
            let folder = if let Some(folder) = found {
                folder.file
            } else {
                Self::create_folder(&current, seg, access_token).await?
            };
            current = folder.id;
        }
        Ok(current)
    }

    async fn get_or_create_folder(
        parent_id: String,
        access_token: &str,
        folder_name: &str,
    ) -> Result<DriveFile, CommonServiceError> {
        let found = Self::find_child_folder(&parent_id, folder_name, access_token).await?;
        let folder = if let Some(folder) = found {
            folder.file
        } else {
            Self::create_folder(&parent_id, folder_name, access_token).await?
        };

        Ok(folder)
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
                "removeParents='{old_parent_id}'&addParents='{new_parent_id}'"
            ))
            .patch::<DriveFile, CommonServiceError>()
            .await
    }

    pub fn ensure_folder_on_remote(
        root_folder_id: String,
        mut root_dir: PathBuf,
        access_token: String,
        local_folder_path: PathBuf,
    ) -> Option<Task<Message>> {
        if !local_folder_path.is_dir() {
            return None;
        }

        let relative_path = local_folder_path.strip_prefix(&root_dir).unwrap();
        let current_drive_id = root_folder_id;

        let mut segments = vec![];
        for segment in relative_path.components() {
            match segment {
                std::path::Component::Normal(os_str) => {
                    root_dir = root_dir.join(os_str);
                    segments.push((root_dir.clone(), os_str.to_string_lossy().to_string()));
                }
                _ => {}
            }
        }

        if segments.is_empty() {
            return None;
        }

        // iter over the paths and either find or create them. Send UploadFinished so internal file_index can be updated
        let tasks = unfold(
            (current_drive_id, segments.into_iter(), access_token),
            |(mut current_drive_id, mut iter, access_token)| async move {
                let (path, folder_name) = iter.next()?;
                let get_folder_result = Self::get_or_create_folder(
                    current_drive_id.clone(),
                    &access_token.clone(),
                    &folder_name,
                )
                .await
                .map_err(|e| SyncError::from(e));

                if let Ok(folder) = &get_folder_result {
                    current_drive_id = folder.id.clone();
                    let msg = Message::Sync(SyncMessage::UploadFinished {
                        path,
                        result: get_folder_result,
                    });
                    let next_state = (current_drive_id, iter, access_token);
                    Some((msg, next_state))
                } else {
                    None
                }
            },
        );

        Some(Task::stream(tasks))
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
        let bytes = std::fs::read(local_file_path).map_err(|e| SyncError::Io(format!("{}", e)))?;

        let location = Self::start_resumable_create(&file_name, &parent_folder_id, &access_token)
            .await
            .map_err(SyncError::from)?;

        let created = Self::put_resumable_bytes(location, bytes, &access_token)
            .await
            .map_err(SyncError::from)?;
        Ok(created)
    }

    pub async fn upload_local_file(
        local_file_path: &Path,
        drive_parent_folder_id: &str,
        access_token: &str,
    ) -> Result<DriveFile, SyncError> {
        if !local_file_path.exists() {
            return Err(SyncError::Io(format!(
                "File missing: {}",
                local_file_path.display()
            )));
        }
        if !local_file_path.is_file() {
            return Err(SyncError::Io(format!(
                "Not a file: {}",
                local_file_path.display()
            )));
        }

        let file_name = local_file_path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| SyncError::Io("Invalid filename (non-UTF8)".to_string()))?;

        let bytes = std::fs::read(local_file_path).map_err(|e| SyncError::Io(format!("{}", e)))?;

        let existing = Self::find_by_name(drive_parent_folder_id, file_name, access_token, false)
            .await
            .map_err(SyncError::from)?;

        match existing {
            Some(file) => {
                println!("existing");
                let location = Self::start_resumable_update(&file.file.id, access_token)
                    .await
                    .map_err(SyncError::from)?;
                println!("location {location}");
                let updated = Self::put_resumable_bytes(location, bytes, access_token)
                    .await
                    .map_err(SyncError::from)?;
                Ok(updated)
            }
            None => {
                println!("resumable");
                let location =
                    Self::start_resumable_create(file_name, drive_parent_folder_id, access_token)
                        .await
                        .map_err(SyncError::from)?;

                let created = Self::put_resumable_bytes(location, bytes, access_token)
                    .await
                    .map_err(SyncError::from)?;
                Ok(created)
            }
        }
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
        url.set_query(Some("alt=media"));

        let bytes = HTTP
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(CommonServiceError::from)?
            .error_for_status()
            .map_err(CommonServiceError::from)?
            .bytes()
            .await
            .map_err(CommonServiceError::from)?;

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
            .map_err(CommonServiceError::from)?
            .error_for_status()
            .map_err(CommonServiceError::from)?;

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
            .map_err(CommonServiceError::from)?
            .error_for_status()
            .map_err(CommonServiceError::from)?;

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
            .map_err(CommonServiceError::from)?
            .error_for_status()
            .map_err(CommonServiceError::from)?
            .json::<DriveFile>()
            .await
            .map_err(CommonServiceError::from)
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
