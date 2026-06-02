use iced::Task;
use serde::Serialize;

use crate::{
    ArchiveClient, UserState,
    app::message::{CommonServiceError, Message, UnixSocketCommand},
    services::drive::DriveRevision,
};

#[derive(Debug, Serialize, Clone)]
pub struct FileWithRevision {
    #[serde(rename = "fileId")]
    pub(crate) file_id: String,
    #[serde(flatten)]
    pub(crate) revision: DriveRevision,
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
                    revisions_cache,
                    ..
                },
                UnixSocketCommand::GetFileRevisions {
                    path,
                    force_refresh,
                    sender,
                },
            ) => ArchiveClient::get_file_revisions_task(
                path,
                force_refresh,
                sender,
                root_folder_id.clone(),
                resolver.clone(),
                user_data.access_token.clone(),
                revisions_cache.clone(),
            ),
            (
                UserState::OrgSynced {
                    user_data,
                    resolver,
                    root_folder_id,
                    revisions_cache,
                    ..
                },
                UnixSocketCommand::ShowAllRevisions { path },
            ) => {
                let revisions_task = ArchiveClient::show_all_revisions_task(
                    path,
                    root_folder_id.clone(),
                    resolver.clone(),
                    user_data.access_token.clone(),
                    revisions_cache.clone(),
                );
                let restore_task = iced::window::latest().then(|maybe_id| {
                    maybe_id
                        .map(|id| {
                            Task::batch([
                                iced::window::minimize::<Message>(id, false),
                                iced::window::gain_focus::<Message>(id),
                            ])
                        })
                        .unwrap_or(Task::none())
                });
                Task::batch([restore_task, revisions_task])
            }
            (
                UserState::OrgSynced {
                    user_data,
                    resolver,
                    root_dir,
                    ..
                },
                UnixSocketCommand::DownloadRevision {
                    file_id,
                    revision_id,
                    modified_time,
                },
            ) => {
                let (tx, _rx) = tokio::sync::oneshot::channel();
                ArchiveClient::download_file_at_path_task(
                    file_id,
                    revision_id,
                    modified_time,
                    resolver.clone(),
                    root_dir.clone(),
                    user_data.access_token.clone(),
                    Box::new(tx),
                )
            }
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
                if matches!(err, CommonServiceError::TokenExpired(..)) {
                    self.app
                        .pending_intents
                        .push(crate::app::state::Intent::ExternalRequest { cmd: *cmd });
                }

                self.handle_error(crate::app::message::GlobalError::Common(err))
            }
            _ => Task::none(),
        }
    }
}
