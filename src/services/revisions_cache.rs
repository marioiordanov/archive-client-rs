use std::{
    ops::Add,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;

use crate::services::drive::DriveRevision;

const TTL_DURATION: Duration = Duration::from_secs(5 * 2); // 5*2 secs

#[derive(Default)]
pub(crate) struct CachedRevisions {
    live_until: Duration,
    sorted_revisions: Vec<DriveRevision>,
}

#[derive(Default, Clone)]
pub(crate) struct Cache {
    file_id_to_revisions: Arc<DashMap<String, CachedRevisions>>,
}

impl Cache {
    pub fn get(&self, file_id: String) -> Option<Vec<DriveRevision>> {
        match self.file_id_to_revisions.entry(file_id) {
            dashmap::Entry::Occupied(e)
                if e.get().live_until >= SystemTime::now().duration_since(UNIX_EPOCH).unwrap() =>
            {
                Some(e.get().sorted_revisions.clone())
            }
            dashmap::Entry::Occupied(e) => {
                e.remove();
                None
            }
            dashmap::Entry::Vacant(vacant_entry) => None,
        }
    }

    pub fn insert(&self, file_id: String, sorted_revisions: Vec<DriveRevision>) {
        let cached = CachedRevisions {
            live_until: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .add(TTL_DURATION),
            sorted_revisions,
        };

        self.file_id_to_revisions.insert(file_id, cached);
    }
}
