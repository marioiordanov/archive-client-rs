use std::{path::Path, str::FromStr};

use reqwest::header;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
	app::message::{CommonServiceError, SyncError},
	constants::{FILES_UPLOAD_URL, FILES_URL},
	services::http::HttpService,
	HTTP,
};

pub struct DriveService;

#[derive(Debug, Deserialize)]
struct FileListResponse {
	files: Vec<DriveFile>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DriveFile {
	pub id: String,
	pub name: String,
	#[serde(rename = "mimeType")]
	pub mime_type: Option<String>,
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

impl DriveService {
	pub async fn find_child_folder(
		parent_id: &str,
		folder_name: &str,
		access_token: &str,
	) -> Result<Option<DriveFile>, CommonServiceError> {
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
				folder
			} else {
				Self::create_folder(&current, seg, access_token).await?
			};
			current = folder.id;
		}
		Ok(current)
	}

	pub async fn find_file_by_name(
		parent_id: &str,
		file_name: &str,
		access_token: &str,
	) -> Result<Option<DriveFile>, CommonServiceError> {
		let query = format!(
			"q='{}' in parents and trashed=false and mimeType!='application/vnd.google-apps.folder' and name='{}'&fields=files(id,name,mimeType)",
			parent_id,
			escape_drive_query_string(file_name)
		);

		let resp = HttpService::<()>::new(FILES_URL)
			.auth(access_token)
			.query(&query)
			.get::<FileListResponse, CommonServiceError>()
			.await?;

		Ok(resp.files.into_iter().next())
	}

	pub async fn upload_local_file(
		local_file_path: &Path,
		drive_parent_folder_id: &str,
		access_token: &str,
	) -> Result<(), SyncError> {
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

		let mime = mime_guess::from_path(local_file_path)
			.first_or_octet_stream()
			.essence_str()
			.to_string();

		let bytes = std::fs::read(local_file_path)
			.map_err(|e| SyncError::Io(format!("{}", e)))?;

		let existing = Self::find_file_by_name(drive_parent_folder_id, file_name, access_token)
			.await
			.map_err(SyncError::from)?;

		match existing {
			Some(file) => {
				let location = Self::start_resumable_update(&file.id, access_token)
					.await
					.map_err(SyncError::from)?;
				Self::put_resumable_bytes(location, bytes, &mime, access_token)
					.await
					.map_err(SyncError::from)?;
				Ok(())
			}
			None => {
				let location = Self::start_resumable_create(file_name, drive_parent_folder_id, access_token)
					.await
					.map_err(SyncError::from)?;
				Self::put_resumable_bytes(location, bytes, &mime, access_token)
					.await
					.map_err(SyncError::from)?;
				Ok(())
			}
		}
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
			.header(header::HeaderName::from_static("x-upload-content-type"), "application/octet-stream")
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
			.header(header::HeaderName::from_static("x-upload-content-type"), "application/octet-stream")
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
		mime: &str,
		access_token: &str,
	) -> Result<(), CommonServiceError> {
		HTTP
			.put(location)
			.bearer_auth(access_token)
			.header(header::CONTENT_TYPE, mime)
			.body(bytes)
			.send()
			.await
			.map_err(CommonServiceError::from)?
			.error_for_status()
			.map_err(CommonServiceError::from)
			.map(|_| ())
	}
}

fn escape_drive_query_string(value: &str) -> String {
	// Drive query strings use single quotes. Escape with backslash.
	value.replace('\\', "\\\\").replace('\'', "\\'")
}
