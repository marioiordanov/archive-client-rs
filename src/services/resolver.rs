use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use iced::widget::sensor::Key;
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    app::message::SyncError,
    services::{
        drive::DriveService,
        file_index::FileIndex,
    },
};

#[derive(Default)]
pub(crate) struct Resolver {
    local_storage: Arc<tokio::sync::RwLock<FileIndex>>,
    root_dir: PathBuf,
}

//
impl Resolver {
    pub fn new(root_dir: PathBuf, file_index: FileIndex) -> Self {
        Self {
            local_storage: Arc::new(RwLock::new(file_index)),
            root_dir,
        }
    }

    pub(crate) async fn get_file_index(&self) -> RwLockReadGuard<FileIndex> {
        self.local_storage.read().await
    }

    pub(crate) async fn resolve_path_and_create_intermediaries(
        &self,
        path: PathBuf,
        root_dir_id: String,
        access_token: String,
    ) -> Result<String, SyncError> {
        let resolve_path_result = self
            .resolve_path(path.clone(), root_dir_id.clone(), access_token.clone())
            .await;
        match resolve_path_result {
            Ok(file_id) => Ok(file_id),
            Err(SyncError::PathDoesntExistOnRemote(intermediary_path))
                if path != intermediary_path =>
            {
                let intermediate_parent = intermediary_path
                    .parent()
                    .expect("intermediary path parent must not be root path");
                let file_name = intermediary_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                if intermediate_parent.eq(&self.root_dir) {
                    let drive_folder =
                        DriveService::create_folder(&root_dir_id, &file_name, &access_token)
                            .await?;
                    self.local_storage
                        .write()
                        .await
                        .put_file_id(intermediary_path, drive_folder.id);

                    Box::pin(self.resolve_path_and_create_intermediaries(
                        path,
                        root_dir_id,
                        access_token,
                    ))
                    .await
                } else {
                    let parent_id = Box::pin(self.resolve_path(
                        intermediate_parent.to_path_buf(),
                        root_dir_id.clone(),
                        access_token.clone(),
                    ))
                    .await?;
                    let intermediary_drive_file =
                        DriveService::create_folder(&parent_id, &file_name, &access_token).await?;
                    self.local_storage
                        .write()
                        .await
                        .put_file_id(intermediary_path, intermediary_drive_file.id);

                    Box::pin(self.resolve_path_and_create_intermediaries(
                        path,
                        root_dir_id,
                        access_token,
                    ))
                    .await
                }
            }
            Err(e) => Result::Err(e),
        }
    }
    pub(crate) async fn resolve_path(
        &self,
        path: PathBuf,
        mut root_dir_id: String,
        access_token: String,
    ) -> Result<String, SyncError> {
        let is_folder = path.is_dir();
        if let Some(file_id) = self.local_storage.read().await.get_file_id(&path) {
            Ok(file_id.clone())
        } else {
            let mut ancestors = path
                .strip_prefix(&self.root_dir)
                .expect("Mismatch between path and root_dir")
                .ancestors()
                .into_iter()
                .collect::<Vec<&Path>>();
            ancestors.reverse();

            // start from the second element, because the first is the root_dir directory and it is returned as empty string
            for parent in ancestors[1..ancestors.len() - 1].iter() {
                let current_path = self.root_dir.join(parent);
                if let Some(file_id) = self.local_storage.read().await.get_file_id(&current_path) {
                    root_dir_id = file_id.clone();
                } else {
                    let object_name = parent.file_name().unwrap().to_string_lossy().to_string(); // safe to use unwrap, because earlier we made sure that its not .. path via strip_prefix
                    let maybe_file_id =
                        DriveService::find_by_name(&root_dir_id, &object_name, &access_token, true)
                            .await?;
                    if let Some(file_id) = maybe_file_id {
                        self.local_storage
                            .write()
                            .await
                            .put_file_id(current_path.clone(), file_id.file.id.clone());
                        root_dir_id = file_id.file.id;
                    } else {
                        return Err(SyncError::PathDoesntExistOnRemote(current_path));
                    }
                }
            }

            let file_name = path.file_name().unwrap().to_string_lossy().to_string();

            if let Some(drive_file) =
                DriveService::find_by_name(&root_dir_id, &file_name, &access_token, is_folder)
                    .await?
            {
                self.local_storage
                    .write()
                    .await
                    .put_file_id(path, drive_file.file.id.clone());
                Ok(drive_file.file.id)
            } else {
                Err(SyncError::PathDoesntExistOnRemote(path))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };

    use iced::advanced::graphics::text::cosmic_text::skrifa::raw::tables::loca;
    use tokio::sync::RwLock;

    use crate::services::{auth::AuthService, file_index::FileIndex, resolver::Resolver};

    #[tokio::test]
    async fn get_ancestors() {
        let refresh = AuthService::refresh_access_token("REFRESH_TOKEN".into()).await.unwrap();
        let root = PathBuf::from("/Users/mario/Projects/archive-client-rs/app-data");
        let path = PathBuf::from("/Users/mario/Projects/archive-client-rs/app-data/mario/aaaa/x");
        let mut local_storage = FileIndex::default();
        local_storage.put_file_id(
            PathBuf::from("/Users/mario/Projects/archive-client-rs/app-data/dd"),
            "1oyhozaAxkXISXfVhzEe_4CM3AJv-DKem".to_string(),
        );

        let r = Resolver {
            local_storage: Arc::new(RwLock::new(local_storage)),
            root_dir: root,
        };

        let id = r
            .resolve_path_and_create_intermediaries(
                path,
                "18NTDkndn_ESjsActq-6CRFUiMvfHTLWL".into(),
                refresh.access_token,
            )
            .await;
        println!("{id:?}");
        println!("{:?}", r.local_storage.read().await);
    }
}
