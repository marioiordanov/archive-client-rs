use std::{path::{Path, PathBuf}, sync::Arc};

use iced::Task;
use serde::Serialize;

use crate::{
    ArchiveClient, UserState,
    app::message::{Message, UnixSocketCommand},
    services::drive::{DriveRevision, DriveService},
};

#[derive(Debug, Serialize)]
pub struct FileWithRevision {
    file_id: String,
    #[serde(flatten)]
    revision: DriveRevision
}

impl ArchiveClient {
    pub fn handle_unix_socket_commands(&self, command: UnixSocketCommand) -> Task<Message> {
        match (&self.app.user_state, command) {
            (
                UserState::OrgSynced {
                    user_data,
                    resolver,
                    root_folder_id,
                    ..
                },
                UnixSocketCommand::GetFileRevisions { path, sender },
            ) => {
                let access_token = user_data.access_token.clone();
                let resolver = resolver.clone();
                let root_folder_id = root_folder_id.clone();

                Task::perform(
                    async move {
                        let id = resolver
                            .resolve_path(path, root_folder_id.clone(), access_token.clone())
                            .await
                            .unwrap();
                        let revisions = DriveService::list_revisions(&id, &access_token)
                            .await
                            .unwrap()
                            .into_iter().map(|r| FileWithRevision {file_id: id.clone(), revision: r}).collect();
                        sender.send(revisions);
                    },
                    |_| {
                        Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted {
                            success: true,
                        })
                    },
                )
            }
            (
                UserState::OrgSynced { user_data, resolver, root_dir, .. },
                UnixSocketCommand::DownloadFileAtPath {
                    file_id,
                    revision_id,
                    modified_time
                },
            ) => {
                let access_token = user_data.access_token.clone();
                let resolver = resolver.clone();
                let root_dir = root_dir.clone();
                Task::perform(async move {
                   if let Some(file_name) = resolver.get_object_name(&file_id).await {
                        let file_contents = DriveService::download_revision(&file_id, &revision_id, &access_token).await.unwrap();
                        let file_name = format!("{modified_time}-{file_name}");
                        let parent = root_dir.join(".archived");
                        if !parent.exists() {
                            tokio::fs::create_dir(&parent).await;
                        }

                        tokio::fs::write(parent.join(file_name), file_contents).await.unwrap();
                   }

                }, |_| Message::UnixSocket(UnixSocketCommand::UnixCommandCompleted { success: true }))
            }
            _ => Task::none(),
        }
    }
}
