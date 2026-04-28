use std::{
    path::{self, Path, PathBuf},
    sync::Arc,
};

use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    app::message::SyncError,
    services::{drive::DriveService, file_index::FileIndex},
};

#[derive(Default, Clone)]
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

    pub(crate) async fn move_object_in_file_index(
        &self,
        _from: PathBuf,
        to: PathBuf,
        file_id: String,
    ) {
        let mut file_index = self.local_storage.write().await;

        file_index.put_file_id(to, file_id);
        file_index.reload_cache_by_path();
    }

    pub(crate) async fn update_file_index(&self, path: PathBuf, file_id: String) {
        self.local_storage.write().await.put_file_id(path, file_id);
    }

    pub(crate) async fn remove_from_file_index(&self, path: PathBuf) {
        let mut file_index = self.local_storage.write().await;
        file_index.remove(path);
        file_index.reload_cache_by_path();
    }

    pub(crate) async fn save_on_local(&self) {
        self.local_storage.read().await.save();
    }

    pub(crate) fn try_save_on_local(&self) {
        if let Ok(file_index) = self.local_storage.try_read() {
            file_index.save();
        }
    }

    pub async fn get_object_name(&self, object_id: &String) -> Option<String>{
        self.local_storage.read().await.get_object_name(object_id).cloned()
    }

    /// Resolves `path` to its Drive file ID, creating any missing ancestor folders along the way.
    ///
    /// For files: ensures all parent folders exist on Drive, then returns the file's ID if it exists.
    /// For directories: creates all missing folders including `path` itself, then returns its ID.
    ///
    /// Returns `PathDoesNotExistOnRemote(path)` only when the path is a file that doesn't exist on
    /// Drive yet — all its ancestors will have been created by then.
    pub(crate) async fn resolve_and_create_missing_ancestors(
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
            Err(SyncError::PathDoesNotExistOnRemote(intermediary_path))
                if path != intermediary_path || path.is_dir() =>
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

                    Box::pin(self.resolve_and_create_missing_ancestors(
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

                    Box::pin(self.resolve_and_create_missing_ancestors(
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
        if path.eq(&self.root_dir) {
            return Ok(root_dir_id);
        }

        if let Some(file_id) = self.local_storage.read().await.get_file_id(path.clone()) {
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
            // and dont use the last element which is the full path
            for parent in ancestors.iter().take(ancestors.len() - 1).skip(1) {
                let current_path = self.root_dir.join(parent);
                if let Some(file_id) = self
                    .local_storage
                    .read()
                    .await
                    .get_file_id(current_path.clone())
                {
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
                            .put_file_id(current_path, file_id.file.id.clone());
                        root_dir_id = file_id.file.id;
                    } else {
                        return Err(SyncError::PathDoesNotExistOnRemote(current_path));
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
                Err(SyncError::PathDoesNotExistOnRemote(path))
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

    use tokio::sync::RwLock;

    use crate::services::{auth::AuthService, file_index::FileIndex, resolver::Resolver};

    #[test]
    fn check_sth() {
        let path =
            PathBuf::from("/Users/mario/Projects/archive-client-rs/test-folder/mario/aaaa/x");
        let ancestors = path.ancestors().into_iter().collect::<Vec<&Path>>();

        let ancestors = vec![1, 2];
        let ancestors = ancestors.iter().take(ancestors.len() - 1).skip(1);
        let ancestors = ancestors.cloned().collect::<Vec<i32>>();
        println!("{ancestors:?}");
    }

    #[tokio::test]
    async fn get_ancestors() {
        let refresh = AuthService::refresh_access_token("REFRESH_TOKEN".into()).await.unwrap();
        let root = PathBuf::from("/Users/mario/Projects/archive-client-rs/test-folder");
        let path = PathBuf::from("/Users/mario/Projects/archive-client-rs/test-folder/mario/aaaa");
        let mut local_storage = FileIndex::default();
        local_storage.put_file_id(
            PathBuf::from("/Users/mario/Projects/archive-client-rs/test-folder/dd"),
            "1oyhozaAxkXISXfVhzEe_4CM3AJv-DKem".to_string(),
        );

        let r = Resolver {
            local_storage: Arc::new(RwLock::new(local_storage)),
            root_dir: root,
        };

        let id = r
            .resolve_path(
                path,
                "18NTDkndn_ESjsActq-6CRFUiMvfHTLWL".into(),
                refresh.access_token,
            )
            .await;
        println!("{id:?}");
        println!("{:?}", r.local_storage.read().await);
    }
}
