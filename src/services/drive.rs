use std::{fmt, path::PathBuf, str::FromStr};

use chrono::NaiveDateTime;
use reqwest::header;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use url::Url;

use crate::{
    HTTP,
    app::message::{CommonServiceError, SyncError},
    constants::{FILES_UPLOAD_URL, FILES_URL},
    services::http::HttpService,
};

pub struct DriveService;

#[derive(Debug, Deserialize)]
struct FileListResponse {
    files: Vec<DriveFileWithParent>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct DriveFileWithParent {
    #[serde(flatten)]
    pub(crate) file: DriveFile,
    #[serde(
        rename = "parents",
        deserialize_with = "deserialize_parents_into_parent"
    )]
    parent: String,
}

pub fn deserialize_modified_time_from_rf339_to_local_time<'de, D>(
    deserializer: D,
) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let time = String::deserialize(deserializer)?;

    Ok(chrono::DateTime::parse_from_rfc3339(&time)
        .map_err(|_e| serde::de::Error::custom("Datetime not in RFC339 format"))?
        .naive_local())
}

fn serialize_naive_datetime<S>(dt: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // %c format specifier: Sun Jul  8 00:34:60 2001 Locale’s date and time (e.g., Thu Mar  3 23:05:25 2005).
    serializer.serialize_str(&format!("{}", dt.format("%c")))
}
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct DriveRevision {
    pub id: String,
    #[serde(
        rename = "modifiedTime",
        deserialize_with = "deserialize_modified_time_from_rf339_to_local_time",
        serialize_with = "serialize_naive_datetime"
    )]
    pub modified_time: NaiveDateTime,
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

#[derive(Deserialize, Debug, Clone, Serialize)]
pub(crate) struct FolderResponse {
    id: String,
    name: String,
    #[serde(
        rename = "parents",
        deserialize_with = "deserialize_parents_into_parent"
    )]
    parent: String,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct FolderResponseList {
    files: Vec<FolderResponse>,
}

impl DriveService {
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?;

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

    pub async fn fetch_activity_log(
        ancestor_id: &str,
        access_token: &str,
        page_token: Option<String>,
    ) -> Result<(Vec<Activity>, Option<String>), CommonServiceError> {
        use crate::constants::ACTIVITY_URL;

        let mut body = json!({
            "ancestorName": format!("items/{ancestor_id}"),
            "pageSize": 20,
            "consolidationStrategy": { "none": {} },
        });

        if let Some(token) = page_token {
            body["pageToken"] = json!(token);
        }

        let resp = HTTP
            .post(ACTIVITY_URL)
            .bearer_auth(access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .error_for_status()
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?
            .json::<Activities>()
            .await
            .map_err(|e| CommonServiceError::from((e, access_token.to_string())))?;

        Ok((resp.activities, resp.next_page_token))
    }
}

fn escape_drive_query_string(value: &str) -> String {
    // Drive query strings use single quotes. Escape with backslash.
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum Detail {
    Edit(Edit),
    Create(Create),
    Move(Move),
    Rename(Rename),
    Delete(Delete),
    Restore(Restore),
    PermissionChange(PermissionChange),
    Comment(serde_json::Value),
    DlpChange(serde_json::Value),
    Reference(serde_json::Value),
    SettingsChange(serde_json::Value),
    AppliedLabelChange(serde_json::Value),
}

#[derive(Deserialize, Debug, Clone)]
pub struct Edit {}

#[derive(Deserialize, Debug, Clone)]
pub struct Create {
    // e.g. {"upload": {}} — another externally tagged enum
    pub upload: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Move {
    pub added_parents: Option<Vec<Parent>>,
    pub removed_parents: Option<Vec<Parent>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Parent {
    pub drive_item: DriveItem,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DriveItem {
    pub name: String,
    pub title: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Delete {
    #[serde(rename = "type")]
    pub delete_type: String, // "PERMANENT_DELETE"
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Rename {
    pub old_title: String,
    pub new_title: String,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct Restore {
    #[serde(rename = "type")]
    pub restore_type: String, // "UNTRASH"
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionChange {
    pub added_permissions: Option<Vec<serde_json::Value>>,
    pub removed_permissions: Option<Vec<serde_json::Value>>,
}

// --- Actor ---

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum Actor {
    User(User),
    Anonymous(serde_json::Value),
    Impersonation(serde_json::Value),
    System(serde_json::Value),
    Administrator(serde_json::Value),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, clippy::enum_variant_names)]
pub enum User {
    KnownUser(KnownUser),
    DeletedUser(serde_json::Value),
    UnknownUser(serde_json::Value),
}

// V3 GoogleDrive API doesnt support multiple parents
fn deserialize_person_name_by_stripping_prefix<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let person_name = String::deserialize(deserializer)?;
    // its not possible for a file to not have parent
    if let Some(stripped_name) = person_name.strip_prefix("people/") {
        Ok(stripped_name.to_string())
    } else {
        Ok(person_name)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KnownUser {
    #[serde(deserialize_with = "deserialize_person_name_by_stripping_prefix")]
    pub person_name: String,
    pub is_current_user: Option<bool>,
}

// --- Target ---

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum Target {
    DriveItem(TargetDriveItem),
    Drive(Drive),
    FileComment(serde_json::Value),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct TargetDriveItem {
    pub name: String,
    pub title: String,
    pub mime_type: Option<String>,
    pub owner: Option<serde_json::Value>,
    pub drive_file: Option<serde_json::Value>,
    pub drive_folder: Option<DriveFolder>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DriveFolder {
    #[serde(rename = "type")]
    pub folder_type: String, // "MY_DRIVE_ROOT" | "SHARED_DRIVE_ROOT" | "STANDARD_FOLDER"
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Drive {
    pub name: String,
    pub title: String,
    pub root: Option<TargetDriveItem>,
}

// --- Display ---

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Detail::Edit(_) => write!(f, "edited"),
            Detail::Create(c) => {
                if c.upload.is_some() {
                    write!(f, "uploaded")
                } else {
                    write!(f, "created")
                }
            }
            Detail::Move(m) => {
                let from = m
                    .removed_parents
                    .as_deref()
                    .and_then(|v| v.first())
                    .map(|p| p.drive_item.title.as_str());
                let to = m
                    .added_parents
                    .as_deref()
                    .and_then(|v| v.first())
                    .map(|p| p.drive_item.title.as_str());
                match (from, to) {
                    (Some(f_), Some(t)) => write!(f, "moved from \"{f_}\" to \"{t}\""),
                    (Some(f_), None) => write!(f, "moved out of \"{f_}\""),
                    (None, Some(t)) => write!(f, "moved to \"{t}\""),
                    (None, None) => write!(f, "moved"),
                }
            }
            Detail::Rename(r) => write!(f, "renamed \"{}\" → \"{}\"", r.old_title, r.new_title),
            Detail::Delete(d) => {
                if d.delete_type == "PERMANENT_DELETE" {
                    write!(f, "permanently deleted")
                } else {
                    write!(f, "deleted")
                }
            }
            Detail::Restore(_) => write!(f, "restored"),
            Detail::PermissionChange(p) => {
                let added = p.added_permissions.as_deref().map(|v| v.len()).unwrap_or(0);
                let removed = p
                    .removed_permissions
                    .as_deref()
                    .map(|v| v.len())
                    .unwrap_or(0);
                match (added, removed) {
                    (a, 0) => write!(f, "granted {a} permission(s)"),
                    (0, r) => write!(f, "revoked {r} permission(s)"),
                    (a, r) => write!(f, "changed permissions (+{a}/-{r})"),
                }
            }
            Detail::Comment(_) => write!(f, "commented"),
            Detail::DlpChange(_) => write!(f, "triggered DLP change"),
            Detail::Reference(_) => write!(f, "referenced in external app"),
            Detail::SettingsChange(_) => write!(f, "changed settings"),
            Detail::AppliedLabelChange(_) => write!(f, "changed label"),
        }
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::User(u) => write!(f, "{u}"),
            Actor::Anonymous(_) => write!(f, "anonymous user"),
            Actor::Impersonation(_) => write!(f, "impersonator"),
            Actor::System(_) => write!(f, "system"),
            Actor::Administrator(_) => write!(f, "administrator"),
        }
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            User::KnownUser(k) => {
                if k.is_current_user.unwrap_or(false) {
                    write!(f, "you")
                } else {
                    // person_name is "people/<id>"; show the id portion
                    let name = k
                        .person_name
                        .strip_prefix("people/")
                        .unwrap_or(&k.person_name);
                    write!(f, "user:{name}")
                }
            }
            User::DeletedUser(_) => write!(f, "deleted user"),
            User::UnknownUser(_) => write!(f, "unknown user"),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::DriveItem(i) => write!(f, "\"{}\"", i.title),
            Target::Drive(d) => write!(f, "drive \"{}\"", d.title),
            Target::FileComment(_) => write!(f, "a file comment"),
        }
    }
}

impl fmt::Display for Activity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // who
        let actors = if self.actors.is_empty() {
            "unknown".to_string()
        } else {
            self.actors
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        // what
        let action = &self.primary_action_detail;

        // which files
        let targets = if self.targets.is_empty() {
            "unknown target".to_string()
        } else {
            self.targets
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        write!(f, "[{}] {} {} {}", self.time, actors, action, targets)
    }
}

impl fmt::Display for ActivityTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActivityTime::Timestamp(ts) => write!(f, "{ts}"),
            ActivityTime::TimeRange(r) => write!(f, "{} – {}", r.start_time, r.end_time),
        }
    }
}

// --- Action ---

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct Action {
    pub detail: Detail,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ActivityTime {
    Timestamp(String),
    TimeRange(TimeRange),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    pub start_time: String,
    pub end_time: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub primary_action_detail: Detail,
    pub actors: Vec<Actor>,
    pub targets: Vec<Target>,
    #[serde(flatten)]
    pub time: ActivityTime,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Activities {
    pub activities: Vec<Activity>,
    pub next_page_token: Option<String>,
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
