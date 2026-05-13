use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use http_body_util::BodyExt;
use iced::Task;
use serde::Serialize;

use crate::{
    ArchiveClient, UserState,
    app::message::{CommonServiceError, Message, SyncError, UnixSocketCommand},
    services::drive::{DriveRevision, DriveService},
};

#[derive(Debug, Serialize)]
pub struct FileWithRevision {
    #[serde(rename = "fileId")]
    file_id: String,
    #[serde(flatten)]
    revision: DriveRevision,
}

impl FileWithRevision {
    pub fn new(file_id: String, revision: DriveRevision) -> Self {
        Self { file_id, revision }
    }
}

impl ArchiveClient {
    pub fn handle_unix_socket_commands(&mut self, command: UnixSocketCommand) -> Task<Message> {
        match (&self.app.user_state, command) {
            (
                UserState::OrgSynced {
                    user_data,
                    resolver,
                    root_folder_id,
                    ..
                },
                UnixSocketCommand::GetFileRevisions { path, sender },
            ) => ArchiveClient::get_file_revisions_task(
                path,
                sender,
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
            ),
            (
                UserState::OrgSynced {
                    user_data,
                    resolver,
                    root_dir,
                    ..
                },
                UnixSocketCommand::DownloadFileAtPath {
                    file_id,
                    revision_id,
                    modified_time,
                    sender,
                },
            ) => ArchiveClient::download_file_at_path_task(
                file_id,
                revision_id,
                modified_time,
                resolver.clone(),
                root_dir.clone(),
                user_data.access_token.clone(),
                sender,
            ),
            (
                UserState::OrgSynced { .. },
                UnixSocketCommand::UnixCommandCompleted {
                    command: Some(cmd),
                    error: Some(err),
                },
            ) => {
                if matches!( err, CommonServiceError::TokenExpired(..)) {
                    self.app.pending_intents.push(crate::app::state::Intent::ExternalRequest { cmd: *cmd });
                }

                self.handle_error(crate::app::message::GlobalError::Common(err))
            }
            _ => Task::none(),
        }
    }
}
