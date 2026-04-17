use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use fs_watcher::Event;

use crate::app::{fs_index::FsIndex, message::SyncAction};

#[derive(Clone, Copy)]
struct BitFlag(u8);
const RENAMED: BitFlag = BitFlag(0b00001);
const MODIFIED: BitFlag = BitFlag(0b00010);
const REMOVED: BitFlag = BitFlag(0b00100);
const CREATED: BitFlag = BitFlag(0b01000);
const ADDED: BitFlag = BitFlag(0b10000);
const REPLACED: BitFlag = BitFlag(0b100000);

impl BitFlag {
    fn has_flag(&self, flag: BitFlag) -> bool {
        (self.0 & flag.0) != 0
    }

    fn is_renamed(&self) -> bool {
        self.has_flag(RENAMED)
    }

    fn is_modified(&self) -> bool {
        self.has_flag(MODIFIED)
    }

    fn is_removed(&self) -> bool {
        self.has_flag(REMOVED)
    }

    fn is_created(&self) -> bool {
        self.has_flag(CREATED)
    }

    fn is_added(&self) -> bool {
        self.has_flag(ADDED)
    }

    fn is_replaced(&self) -> bool {
        self.has_flag(REPLACED)
    }

    fn merge(&mut self, flag: BitFlag) {
        self.0 |= flag.0;
    }

    fn remove_flag(&mut self, flag: BitFlag) {
        self.0 &= !flag.0
    }

    fn has_any_flag(&self) -> bool {
        self.0 > 0
    }
}

impl std::fmt::Debug for BitFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut flags = vec![];
        if self.is_renamed() {
            flags.push("renamed");
        }

        if self.is_added() {
            flags.push("added");
        }

        if self.is_created() {
            flags.push("created");
        }

        if self.is_removed() {
            flags.push("removed");
        }

        if self.is_modified() {
            flags.push("modified");
        }

        if self.is_replaced() {
            flags.push("replaced");
        }

        f.debug_list().entries(flags.iter()).finish()
    }
}

#[derive(Debug)]
struct IdEntry<'a> {
    action: BitFlag,
    initial_path: &'a Path,
    initial_inode: u64,
    is_folder: bool,
    current_path: &'a Path,
    current_inode: u64,
}

pub(crate) struct EventsTransaction<'a> {
    initial_state: &'a FsIndex,
    path_to_id: HashMap<&'a Path, Id>,
    inode_to_id: HashMap<u64, Id>,
    id_to_entry: BTreeMap<Id, IdEntry<'a>>,
    last_id: u32,
}

impl<'a> EventsTransaction<'a> {
    pub(crate) fn new(initial_state: &'a FsIndex) -> Self {
        Self {
            initial_state,
            path_to_id: HashMap::new(),
            inode_to_id: HashMap::new(),
            id_to_entry: BTreeMap::new(),
            last_id: 0,
        }
    }

    fn get_new_id(&mut self) -> u32 {
        self.last_id += 1;
        self.last_id
    }

    pub(crate) fn append_event(&mut self, event: &'a Event) {
        match event {
            Event::FileCreated(path, inode) => self.append_created_file(path.as_path(), *inode),
            Event::FileRemoved(path_buf, inode) => {
                self.append_removed_file(path_buf.as_path(), *inode)
            }
            Event::FileAdded(path_buf, inode) => self.append_added_file(path_buf.as_path(), *inode),
            Event::FileModified(path_buf, old_inode, new_inode) => {
                if old_inode == new_inode {
                    self.append_modified(path_buf.as_path(), *old_inode);
                } else {
                    self.append_modified_with_inode_change(
                        path_buf.as_path(),
                        *old_inode,
                        *new_inode,
                    );
                }
            }
            Event::FileRenamed { from, to, inode } => {
                self.append_renamed_file(from, to, *inode);
            }
            Event::FileReplaced { path, from, to } => {
                self.append_replaced(path.as_path(), *from, *to, false)
            }
            Event::FolderRemoved(path_buf, inode) => {
                self.append_removed_folder(path_buf.as_path(), *inode)
            }
            Event::FolderAdded(path_buf, inode) => {
                self.append_added_folder(path_buf.as_path(), *inode);
            }
            Event::FolderCreated(path_buf, inode) => {
                self.append_created_folder(path_buf.as_path(), *inode);
            }
            Event::FolderRenamed { from, to, inode } => {
                self.append_renamed_folder(from.as_path(), to.as_path(), *inode);
            }
            Event::FolderReplaced { path, from, to } => {
                self.append_replaced(path.as_path(), *from, *to, true);
            }
        }
    }

    fn append_removed_folder(&mut self, path: &'a Path, inode: u64) {
        // go through the tracked children
        let tracked_children: Vec<(&'a Path, Id, bool)> = self
            .path_to_id
            .iter()
            .filter(|(p, _)| **p != path && p.parent() == Some(path))
            .map(|(p, id)| {
                let is_folder = self
                    .id_to_entry
                    .get(id)
                    .map(|e| e.is_folder)
                    .expect("id must be present");

                (*p, *id, is_folder)
            })
            .collect();

        for (_, id, is_folder) in tracked_children {
            let entry = self.id_to_entry.get(&id).expect("id must be present");
            if is_folder {
                self.append_removed_folder(entry.current_path, entry.current_inode);
            } else {
                self.append_removed_file(entry.current_path, entry.current_inode);
            }
        }

        let untracked_children: Vec<(&'a Path, u64, bool)> = self
            .initial_state
            .direct_children
            .get(path)
            .map(|dc| {
                dc.iter()
                    .filter(|(p, i)| {
                        !self.inode_to_id.contains_key(i)
                            && !self.path_to_id.contains_key(p.as_path())
                    })
                    .map(|(p, i)| (p.as_path(), *i, self.initial_state.is_folder(p.as_path())))
                    .collect()
            })
            .unwrap_or_default();

        for (child_path, child_inode, is_folder) in untracked_children {
            if is_folder {
                self.append_removed_folder(child_path, child_inode);
            } else {
                self.append_removed_file(child_path, child_inode);
            }
        }

        let maybe_inode_id = self.inode_to_id.remove(&inode);
        let maybe_path_id = self.path_to_id.remove(path);

        match (maybe_path_id, maybe_inode_id) {
            (None, None) => {
                let id = self.get_new_id();
                let entry = IdEntry {
                    action: REMOVED,
                    is_folder: true,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };

                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
                self.id_to_entry.insert(id, entry);
            }
            (Some(id), Some(inode_id)) if id == inode_id => {
                let mut entry = self.id_to_entry.remove(&id).expect("id must be present");
                if entry.action.is_added() || entry.action.is_created() {
                    // do nothing and it will be removed
                } else {
                    entry.action.merge(REMOVED);
                    // Reassign to a new (higher) ID so the folder's
                    // RemoveFolder sorts after its children's Delete
                    // actions in BTreeMap iteration order.
                    let new_id = self.get_new_id();
                    self.path_to_id.insert(path, new_id);
                    self.inode_to_id.insert(inode, new_id);
                    self.id_to_entry.insert(new_id, entry);
                }
            }
            _ => panic!("Impossible case"),
        }
    }

    // path doesn't change, inode doesnt change
    fn append_modified(&mut self, path: &'a Path, inode: u64) {
        let maybe_path_id = self.path_to_id.get(path);
        let maybe_inode_id = self.inode_to_id.get(&inode);

        match (maybe_path_id, maybe_inode_id) {
            (Some(id), Some(inode_id)) if id == inode_id => {
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                entry.action.merge(MODIFIED);
            }
            (None, None) => {
                let id = self.get_new_id();
                let entry = IdEntry {
                    action: MODIFIED,
                    is_folder: false,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };

                self.id_to_entry.insert(id, entry);
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
            }
            _ => panic!("Impossible case"),
        }
    }

    // path doesn't change, inode changes
    fn append_modified_with_inode_change(
        &mut self,
        path: &'a Path,
        old_inode: u64,
        new_inode: u64,
    ) {
        let maybe_path_id = self.path_to_id.get(path);
        let maybe_inode_id = self.inode_to_id.remove(&old_inode);

        match (maybe_path_id, maybe_inode_id) {
            (Some(id), Some(inode_id)) if *id == inode_id => {
                self.inode_to_id.insert(new_inode, inode_id);
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                entry.action.merge(MODIFIED);
                entry.current_inode = new_inode;
            }
            (None, None) => {
                let entry = IdEntry {
                    action: MODIFIED,
                    is_folder: false,
                    initial_path: path,
                    initial_inode: old_inode,
                    current_path: path,
                    current_inode: new_inode,
                };

                let id = self.get_new_id();
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(new_inode, id);
                self.id_to_entry.insert(id, entry);
            }
            _ => panic!("Impossible case"),
        }
    }

    fn append_replaced(&mut self, path: &'a Path, from_inode: u64, to_inode: u64, is_folder: bool) {
        // When a folder is replaced, the old contents are gone.
        // The fs_watcher will send FileAdded/FolderAdded for the new
        // folder's contents. Remove old children so any that aren't
        // re-added via FileAdded will produce Delete actions.
        if is_folder {
            let tracked_children: Vec<(&'a Path, Id, bool)> = self
                .path_to_id
                .iter()
                .filter(|(p, _)| **p != path && p.parent() == Some(path))
                .map(|(p, id)| {
                    let is_folder = self
                        .id_to_entry
                        .get(id)
                        .map(|e| e.is_folder)
                        .expect("id must be present");
                    (*p, *id, is_folder)
                })
                .collect();

            for (_, id, child_is_folder) in tracked_children {
                let entry = self.id_to_entry.get(&id).expect("id must be present");
                if child_is_folder {
                    self.append_removed_folder(entry.current_path, entry.current_inode);
                } else {
                    self.append_removed_file(entry.current_path, entry.current_inode);
                }
            }

            let untracked_children: Vec<(&'a Path, u64, bool)> = self
                .initial_state
                .direct_children
                .get(path)
                .map(|dc| {
                    dc.iter()
                        .filter(|(p, i)| {
                            !self.inode_to_id.contains_key(i)
                                && !self.path_to_id.contains_key(p.as_path())
                        })
                        .map(|(p, i)| (p.as_path(), *i, self.initial_state.is_folder(p.as_path())))
                        .collect()
                })
                .unwrap_or_default();

            for (child_path, child_inode, child_is_folder) in untracked_children {
                if child_is_folder {
                    self.append_removed_folder(child_path, child_inode);
                } else {
                    self.append_removed_file(child_path, child_inode);
                }
            }
        }

        let maybe_path_id = self.path_to_id.get(path);
        let maybe_inode_id = self.inode_to_id.remove(&from_inode);

        match (maybe_path_id, maybe_inode_id) {
            (None, None) => {
                let entry = IdEntry {
                    action: REPLACED,
                    is_folder,
                    initial_path: path,
                    initial_inode: from_inode,
                    current_path: path,
                    current_inode: to_inode,
                };

                let id = self.get_new_id();
                self.id_to_entry.insert(id, entry);
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(to_inode, id);
            }
            (Some(path_id), Some(id)) if *path_id == id => {
                let entry = self.id_to_entry.get_mut(&id).expect("id must be present");
                entry.action.merge(REPLACED);
                entry.current_inode = to_inode;
                self.inode_to_id.insert(to_inode, id);
            }
            _ => panic!("Impossible case"),
        }
    }

    fn append_renamed_folder(&mut self, from_path: &'a Path, to_path: &'a Path, inode: u64) {
        let maybe_path_id = self.path_to_id.remove(from_path);
        let maybe_inode_id = self.inode_to_id.get(&inode).copied();

        match (maybe_path_id, maybe_inode_id) {
            (Some(id), Some(inode_id)) if inode_id == id => {
                let mut entry = self.id_to_entry.remove(&id).expect("id must be present");
                if entry.initial_path == to_path {
                    entry.action.remove_flag(RENAMED);

                    if entry.action.has_any_flag() {
                        entry.current_path = to_path;
                        self.id_to_entry.insert(id, entry);
                        self.path_to_id.insert(to_path, id);
                    } else {
                        self.inode_to_id.remove(&inode);
                    }
                } else {
                    entry.current_path = to_path;
                    entry.action.merge(RENAMED);
                    self.id_to_entry.insert(id, entry);
                    self.path_to_id.insert(to_path, id);
                }
            }
            (None, None) => {
                let entry = IdEntry {
                    action: RENAMED,
                    is_folder: true,
                    initial_path: from_path,
                    initial_inode: inode,
                    current_path: to_path,
                    current_inode: inode,
                };

                let id = self.get_new_id();
                self.path_to_id.insert(to_path, id);
                self.inode_to_id.insert(inode, id);
                self.id_to_entry.insert(id, entry);
            }
            _ => panic!("Impossible case"),
        }
    }

    // inode stays the same, path changes
    // if path is the same and there is only RENAMED flag, remove from the map
    fn append_renamed_file(&mut self, from_path: &'a Path, to_path: &'a Path, inode: u64) {
        let maybe_path_id = self.path_to_id.remove(from_path);
        let maybe_inode_id = self.inode_to_id.get(&inode).copied();

        match (maybe_path_id, maybe_inode_id) {
            (Some(id), Some(inode_id)) if inode_id == id => {
                let mut entry = self.id_to_entry.remove(&id).expect("id must be present");
                if entry.initial_path == to_path {
                    entry.action.remove_flag(RENAMED);

                    if entry.action.has_any_flag() {
                        entry.current_path = to_path;
                        self.id_to_entry.insert(id, entry);
                        self.path_to_id.insert(to_path, id);
                    } else {
                        self.inode_to_id.remove(&inode);
                    }
                } else {
                    entry.current_path = to_path;
                    entry.action.merge(RENAMED);
                    self.id_to_entry.insert(id, entry);
                    self.path_to_id.insert(to_path, id);
                }
            }
            (None, None) => {
                let entry = IdEntry {
                    action: RENAMED,
                    is_folder: false,
                    initial_path: from_path,
                    initial_inode: inode,
                    current_path: to_path,
                    current_inode: inode,
                };

                let id = self.get_new_id();
                self.path_to_id.insert(to_path, id);
                self.inode_to_id.insert(inode, id);
                self.id_to_entry.insert(id, entry);
            }
            _ => panic!("Impossible case"),
        }
    }

    // it can be move from the folder and then added back with the same name
    // it can be move from the folder and then added back with different name
    // if there is a such a path in the
    // if folder is replaced, all the files of the replaced folder come with Event::FileAdded
    fn append_added_file(&mut self, path: &'a Path, inode: u64) {
        let maybe_inode_id = self.inode_to_id.get(&inode);
        let maybe_path_id = self.path_to_id.get(path);

        match (maybe_path_id, maybe_inode_id) {
            (None, None) => {
                let entry = IdEntry {
                    action: ADDED,
                    is_folder: false,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };

                let id = self.get_new_id();
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
                self.id_to_entry.insert(id, entry);
            }
            (None, Some(id)) => {
                // different name same inode
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                entry.action.remove_flag(REMOVED);
                entry.action.merge(MODIFIED); // file can be modified while it was outside of the watched folder
                entry.action.merge(RENAMED);

                self.path_to_id.remove(entry.current_path);
                self.path_to_id.insert(path, *id);
                entry.current_path = path;
            }
            (Some(id), None) => {
                // same path but different inode, treat it as the user modified the same file, but using tmp file
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                self.inode_to_id.remove(&entry.current_inode);
                entry.current_inode = inode;
                entry.action.remove_flag(REMOVED);
                entry.action.merge(MODIFIED);
                self.inode_to_id.insert(entry.current_inode, *id);
            }
            (Some(id), Some(inode_id)) if id == inode_id => {
                // file returned back,
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                entry.action.remove_flag(REMOVED);
                entry.action.merge(MODIFIED);
            }
            _ => panic!("Impossible case"),
        }
    }

    fn append_added_folder(&mut self, path: &'a Path, inode: u64) {
        let maybe_inode_id = self.inode_to_id.get(&inode);
        let maybe_path_id = self.path_to_id.get(path);

        match (maybe_path_id, maybe_inode_id) {
            (None, None) => {
                let entry = IdEntry {
                    action: ADDED,
                    is_folder: true,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };

                let id = self.get_new_id();
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
                self.id_to_entry.insert(id, entry);
            }
            (None, Some(id)) => {
                // Same inode, different path — folder moved
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                entry.action.remove_flag(REMOVED);
                entry.action.merge(RENAMED);

                self.path_to_id.remove(entry.current_path);
                self.path_to_id.insert(path, *id);
                entry.current_path = path;
            }
            (Some(id), None) => {
                // Same path, different inode — folder replaced
                let entry = self.id_to_entry.get_mut(id).expect("id must be present");
                self.inode_to_id.remove(&entry.current_inode);
                entry.current_inode = inode;
                entry.action.remove_flag(REMOVED);
                entry.action.merge(ADDED);
                self.inode_to_id.insert(inode, *id);
            }
            (Some(id), Some(inode_id)) if id == inode_id => {
                // Same path, same inode — folder returned back
                let mut entry = self.id_to_entry.remove(id).expect("id must be present");
                entry.action.remove_flag(REMOVED);

                if entry.action.has_any_flag() {
                    self.id_to_entry.insert(*id, entry);
                } else {
                    // No flags left — folder came back unchanged, remove tracking
                    self.path_to_id.remove(path);
                    self.inode_to_id.remove(&inode);
                }
            }
            _ => panic!("Impossible case"),
        }
    }

    fn append_removed_file(&mut self, path: &'a Path, inode: u64) {
        let maybe_inode_id = self.inode_to_id.remove(&inode);
        let maybe_path_id = self.path_to_id.remove(path);

        match (maybe_inode_id, maybe_path_id) {
            (None, None) => {
                let entry = IdEntry {
                    action: REMOVED,
                    is_folder: false,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };

                let id = self.get_new_id();
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
                self.id_to_entry.insert(id, entry);
            }
            (Some(id), Some(path_id)) if id == path_id => {
                let mut entry = self.id_to_entry.remove(&id).expect("id must be present");
                if entry.action.is_added() || entry.action.is_created() {
                    // do nothing and it will be removed
                } else {
                    entry.action.merge(REMOVED);
                    self.path_to_id.insert(path, id);
                    self.inode_to_id.insert(inode, id);
                    self.id_to_entry.insert(id, entry);
                }
            }
            _ => panic!("Impossible case"),
        }
    }

    fn append_created_file(&mut self, path: &'a Path, inode: u64) {
        let maybe_inode_id = self.inode_to_id.get(&inode);
        let maybe_path_id = self.path_to_id.get(path);

        match (maybe_path_id, maybe_inode_id) {
            (None, None) => {
                let entry = IdEntry {
                    action: CREATED,
                    is_folder: false,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };
                let id = self.get_new_id();

                self.id_to_entry.insert(id, entry);
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
            }
            // TODO case when creating a file with the same name, that was already deleted
            _ => panic!("Impossible case"),
        }
    }

    fn append_created_folder(&mut self, path: &'a Path, inode: u64) {
        let maybe_inode_id = self.inode_to_id.get(&inode);
        let maybe_path_id = self.path_to_id.get(path);

        match (maybe_path_id, maybe_inode_id) {
            (None, None) => {
                let entry = IdEntry {
                    action: CREATED,
                    is_folder: true,
                    initial_path: path,
                    initial_inode: inode,
                    current_path: path,
                    current_inode: inode,
                };
                let id = self.get_new_id();

                self.id_to_entry.insert(id, entry);
                self.path_to_id.insert(path, id);
                self.inode_to_id.insert(inode, id);
            }
            (Some(id), None) => {
                let entry = self.id_to_entry.get_mut(id).expect("Id must be present");
                entry.action.remove_flag(REMOVED);
                entry.action.merge(CREATED);
                self.inode_to_id.remove(&entry.current_inode);
                entry.current_inode = inode;
            }
            _ => panic!("Impossible case"),
        }
    }

    pub(crate) fn to_sync_actions(self) -> Vec<SyncAction> {
        let mut actions = vec![];
        for entry in self.id_to_entry.into_values() {
            if entry.is_folder {
                // folder specific behavior
                if entry.action.has_flag(REMOVED) {
                    actions.push(SyncAction::RemoveFolder(entry.initial_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(CREATED) {
                    actions.push(SyncAction::EnsureFolder(entry.current_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(ADDED) {
                    actions.push(SyncAction::EnsureFolder(entry.current_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(REPLACED) && entry.action.has_flag(RENAMED) {
                    actions.push(SyncAction::MoveFolder {
                        from: entry.initial_path.to_path_buf(),
                        to: entry.current_path.to_path_buf(),
                    });
                    continue;
                }

                if entry.action.has_flag(REPLACED) {
                    actions.push(SyncAction::EnsureFolder(entry.current_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(RENAMED) {
                    actions.push(SyncAction::MoveFolder {
                        from: entry.initial_path.to_path_buf(),
                        to: entry.current_path.to_path_buf(),
                    });
                    continue;
                }
            } else {
                if entry.action.has_flag(REMOVED) {
                    actions.push(SyncAction::Delete(entry.initial_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(ADDED) {
                    actions.push(SyncAction::Upload(entry.current_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(CREATED) {
                    actions.push(SyncAction::Upload(entry.current_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(MODIFIED) && entry.action.has_flag(RENAMED) {
                    actions.push(SyncAction::MoveAndUpload {
                        from: entry.initial_path.to_path_buf(),
                        to: entry.current_path.to_path_buf(),
                    });
                    continue;
                }

                if entry.action.has_flag(REPLACED) && entry.action.has_flag(RENAMED) {
                    actions.push(SyncAction::MoveAndUpload {
                        from: entry.initial_path.to_path_buf(),
                        to: entry.current_path.to_path_buf(),
                    });
                    continue;
                }

                if entry.action.has_flag(REPLACED) {
                    actions.push(SyncAction::Upload(entry.current_path.to_path_buf()));
                    continue;
                }

                if entry.action.has_flag(RENAMED) {
                    actions.push(SyncAction::Move {
                        from: entry.initial_path.to_path_buf(),
                        to: entry.current_path.to_path_buf(),
                    });
                    continue;
                }

                if entry.action.has_flag(MODIFIED) {
                    actions.push(SyncAction::Upload(entry.current_path.to_path_buf()));
                    continue;
                }
            }
        }

        // --- Post-processing for Google Drive semantics ---

        // 1. Folder move is recursive: drop redundant child moves,
        //    downgrade MoveAndUpload to Upload when the move part is covered.
        let moved_folders: Vec<(std::path::PathBuf, std::path::PathBuf)> = actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::MoveFolder { from, to } => Some((from.clone(), to.clone())),
                _ => None,
            })
            .collect();

        if !moved_folders.is_empty() {
            let is_covered = |from: &Path, to: &Path, exclude_self: bool| {
                moved_folders.iter().any(|(ff, ft)| {
                    (!exclude_self || from != ff.as_path())
                        && from.starts_with(ff)
                        && to.starts_with(ft)
                        && from.strip_prefix(ff) == to.strip_prefix(ft)
                })
            };

            actions = actions
                .into_iter()
                .filter_map(|action| match &action {
                    SyncAction::Move { from, to } if is_covered(from, to, false) => None,
                    SyncAction::MoveAndUpload { to, from } if is_covered(from, to, false) => {
                        Some(SyncAction::Upload(to.clone()))
                    }
                    SyncAction::MoveFolder { from, to } if is_covered(from, to, true) => None,
                    _ => Some(action),
                })
                .collect();
        }

        // 2. Folder deletion is recursive: drop redundant child deletions.
        let removed_folders: Vec<std::path::PathBuf> = actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::RemoveFolder(p) => Some(p.clone()),
                _ => None,
            })
            .collect();

        if !removed_folders.is_empty() {
            actions.retain(|action| {
                let path = match action {
                    SyncAction::Delete(p) | SyncAction::RemoveFolder(p) => p,
                    _ => return true,
                };
                !removed_folders
                    .iter()
                    .any(|folder| path != folder && path.starts_with(folder))
            });
        }

        // 3. Ensure folder operations precede actions on their children.
        //    E.g. MoveFolder { f→g } before Upload("g/b.txt"),
        //    EnsureFolder("g") before Upload("g/x.txt").
        //    Uses stable sort so unrelated actions keep BTreeMap insertion order.
        fn folder_dest(action: &SyncAction) -> Option<&Path> {
            match action {
                SyncAction::MoveFolder { to, .. } => Some(to.as_path()),
                _ => None,
            }
        }

        fn action_path(action: &SyncAction) -> &Path {
            match action {
                SyncAction::Upload(p)
                | SyncAction::Delete(p)
                | SyncAction::EnsureFolder(p)
                | SyncAction::RemoveFolder(p) => p.as_path(),
                SyncAction::MoveAndUpload { to, .. }
                | SyncAction::Move { to, .. }
                | SyncAction::MoveFolder { to, .. } => to.as_path(),
            }
        }

        actions.sort_by(|a, b| {
            use std::cmp::Ordering;

            if let Some(fd) = folder_dest(a) {
                let bp = action_path(b);
                if bp.starts_with(fd) && bp != fd {
                    return Ordering::Less;
                }
            }
            if let Some(fd) = folder_dest(b) {
                let ap = action_path(a);
                if ap.starts_with(fd) && ap != fd {
                    return Ordering::Greater;
                }
            }
            Ordering::Equal
        });

        actions
    }
}

type Id = u32;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap, HashSet},
        env::temp_dir,
        fmt::Debug,
        path::{Path, PathBuf},
        sync::OnceLock,
        time::{SystemTime, UNIX_EPOCH},
    };

    use fs_watcher::Event;
    use rstest::rstest;

    use crate::app::{coalesce::EventsTransaction, fs_index::FsIndex, message::SyncAction};

    fn path_buf(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn run_matrix_case_with_nested_folder_structure(events: Vec<Event>, expected: Vec<SyncAction>) {
        let mut path_to_inode = HashMap::<PathBuf, u64>::new();
        path_to_inode.insert(PathBuf::from("a.txt"), 1);
        path_to_inode.insert(PathBuf::from("b.txt"), 2);
        path_to_inode.insert(PathBuf::from("f"), 3);
        path_to_inode.insert(PathBuf::from("f/b.txt"), 4);
        path_to_inode.insert(PathBuf::from("f/sub"), 5);
        path_to_inode.insert(PathBuf::from("f/sub/deep.txt"), 6);
        path_to_inode.insert(PathBuf::from("f/sub/deep"), 7);
        path_to_inode.insert(PathBuf::from("f/sub/deep/deepest.txt"), 8);

        let mut direct_children = HashMap::<PathBuf, Vec<(PathBuf, u64)>>::new();
        direct_children.insert(
            path_buf("/"),
            vec![
                (path_buf("a.txt"), 1),
                (path_buf("b.txt"), 2),
                (path_buf("f"), 3),
            ],
        );
        direct_children.insert(
            path_buf("f"),
            vec![(path_buf("f/b.txt"), 4), (path_buf("f/sub"), 5)],
        );
        direct_children.insert(
            path_buf("f/sub"),
            vec![(path_buf("f/sub/deep.txt"), 6), (path_buf("f/sub/deep"), 7)],
        );
        direct_children.insert(
            path_buf("f/sub/deep"),
            vec![(path_buf("f/sub/deep/deepest.txt"), 8)],
        );

        let inode_to_path: HashMap<u64, PathBuf> = path_to_inode
            .clone()
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect();

        let mut fs_index = FsIndex {
            path_to_inode,
            inode_to_path,
            direct_children,
        };

        let mut events_processer = EventsTransaction::new(&mut fs_index);
        for e in events.iter() {
            events_processer.append_event(e);
        }

        let result = events_processer.to_sync_actions();
        assert_eq!(result, expected);
    }

    fn run_matrix_case(events: Vec<Event>, expected: Vec<SyncAction>) {
        let mut path_to_inode = HashMap::<PathBuf, u64>::new();
        path_to_inode.insert(PathBuf::from("a.txt"), 1);
        path_to_inode.insert(PathBuf::from("b.txt"), 2);
        path_to_inode.insert(PathBuf::from("f"), 3);
        path_to_inode.insert(PathBuf::from("f/b.txt"), 4);

        let mut direct_children = HashMap::<PathBuf, Vec<(PathBuf, u64)>>::new();
        direct_children.insert(
            path_buf("/"),
            vec![
                (path_buf("a.txt"), 1),
                (path_buf("b.txt"), 2),
                (path_buf("f"), 3),
            ],
        );
        direct_children.insert(path_buf("f"), vec![(path_buf("f/b.txt"), 4)]);

        let inode_to_path: HashMap<u64, PathBuf> = path_to_inode
            .clone()
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect();

        let mut fs_index = FsIndex {
            path_to_inode,
            inode_to_path,
            direct_children,
        };

        let mut events_processer = EventsTransaction::new(&mut fs_index);
        for e in events.iter() {
            events_processer.append_event(e);
        }

        let result = events_processer.to_sync_actions();
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::renamed_then_removed(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRemoved( "c.txt".into(), 1),
        ],
        vec![SyncAction::Delete(path_buf("a.txt"))])
    ]
    #[case::renamed_then_removed_then_added(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRemoved( "c.txt".into(), 1),
            Event::FileAdded("c.txt".into(), 1)
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::renamed_then_removed_then_added_with_different_inode(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRemoved( "c.txt".into(), 1),
            Event::FileAdded("c.txt".into(), 10)
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::replaced_then_removed(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 3,
            },
            Event::FileRemoved( "a.txt".into(), 3),
        ],
        vec![SyncAction::Delete(path_buf("a.txt")) ]
    )]
    #[case::replaced_then_renamed_then_removed(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 3,
            },
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 3,
            },
            Event::FileRemoved( "c.txt".into(), 3),
        ],
        vec![SyncAction::Delete(path_buf("a.txt")) ]
    )]
    fn matrix_multiple_events_and_at_least_one_remove_event(
        #[case] events: Vec<Event>,
        #[case] expected: Vec<SyncAction>,
    ) {
        run_matrix_case(events, expected);
    }

    #[rstest]
    #[case::renamed_then_added_with_old_name_of_renamed_file(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileAdded("a.txt".into(), 10)
        ],
        vec![SyncAction::Move { from: path_buf("a.txt"), to: path_buf("c.txt") }, SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::renamed_then_added_with_old_name_the_renamed_new_file(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileAdded("a.txt".into(), 10),
            Event::FileRenamed { from: "a.txt".into(), to: "b.txt".into(), inode: 10 }
        ],
        vec![SyncAction::Move { from: path_buf("a.txt"), to: path_buf("c.txt") }, SyncAction::Upload(path_buf("b.txt"))]
    )]
    #[case::replaced_and_rename_file(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from:1,
                to: 10,
            },
            Event::FileRenamed { from: "a.txt".into(), to: "b.txt".into(), inode: 10 }
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("b.txt") }]
    )]
    #[case::replaced_file(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 10,
            },
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    fn matrix_multiple_events_without_remove_event(
        #[case] events: Vec<Event>,
        #[case] expected: Vec<SyncAction>,
    ) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Complex multi-event scenarios
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::modified_then_renamed_then_modified(
        vec![
            Event::FileModified("a.txt".into(), 1, 1),
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileModified("c.txt".into(), 1, 1),
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::renamed_then_modified_then_renamed_again(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileModified("c.txt".into(), 1, 1),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 1,
            },
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("d.txt") }]
    )]
    #[case::replaced_then_modified(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 10,
            },
            Event::FileModified("a.txt".into(), 10, 10),
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::replaced_then_renamed_then_modified(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 10,
            },
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 10,
            },
            Event::FileModified("c.txt".into(), 10, 10),
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::rename_chain_three_hops(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 1,
            },
            Event::FileRenamed {
                from: "d.txt".into(),
                to: "e.txt".into(),
                inode: 1,
            },
        ],
        vec![SyncAction::Move { from: path_buf("a.txt"), to: path_buf("e.txt") }]
    )]
    #[case::rename_swap_back_is_noop(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "a.txt".into(),
                inode: 1,
            },
        ],
        vec![]
    )]
    #[case::rename_swap_back_with_modification_in_between(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileModified("c.txt".into(), 1, 1),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "a.txt".into(),
                inode: 1,
            },
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::modified_with_inode_change_then_renamed(
        vec![
            Event::FileModified("a.txt".into(), 1, 10),
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 10,
            },
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::modified_with_inode_change_then_removed(
        vec![
            Event::FileModified("a.txt".into(), 1, 10),
            Event::FileRemoved("a.txt".into(), 10),
        ],
        vec![SyncAction::Delete(path_buf("a.txt"))]
    )]
    #[case::added_then_renamed_then_modified(
        vec![
            Event::FileAdded("c.txt".into(), 20),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 20,
            },
            Event::FileModified("d.txt".into(), 20, 20),
        ],
        vec![SyncAction::Upload(path_buf("d.txt"))]
    )]
    #[case::added_then_removed_is_no_op(
        vec![
            Event::FileAdded("c.txt".into(), 20),
            Event::FileRemoved("c.txt".into(), 20),
        ],
        vec![]
    )]
    #[case::removed_then_added_same_inode(
        vec![
            Event::FileRemoved("a.txt".into(), 1),
            Event::FileAdded("a.txt".into(), 1),
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::removed_then_added_different_inode_same_path(
        vec![
            Event::FileRemoved("a.txt".into(), 1),
            Event::FileAdded("a.txt".into(), 20),
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::removed_then_added_different_path_same_inode(
        vec![
            Event::FileRemoved("a.txt".into(), 1),
            Event::FileAdded("c.txt".into(), 1),
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::two_independent_renames(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRenamed {
                from: "b.txt".into(),
                to: "d.txt".into(),
                inode: 2,
            },
        ],
        vec![
            SyncAction::Move { from: path_buf("a.txt"), to: path_buf("c.txt") },
            SyncAction::Move { from: path_buf("b.txt"), to: path_buf("d.txt") },
        ]
    )]
    #[case::two_independent_modifications(
        vec![
            Event::FileModified("a.txt".into(), 1, 1),
            Event::FileModified("b.txt".into(), 2, 2),
        ],
        vec![
            SyncAction::Upload(path_buf("a.txt")),
            SyncAction::Upload(path_buf("b.txt")),
        ]
    )]
    #[case::rename_one_modify_other(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileModified("b.txt".into(), 2, 2),
        ],
        vec![
            SyncAction::Move { from: path_buf("a.txt"), to: path_buf("c.txt") },
            SyncAction::Upload(path_buf("b.txt")),
        ]
    )]
    #[case::renamed_then_replaced_at_new_path(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileReplaced {
                path: "c.txt".into(),
                from: 1,
                to: 30,
            },
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::multiple_modifications_with_inode_changes(
        vec![
            Event::FileModified("a.txt".into(), 1, 10),
            Event::FileModified("a.txt".into(), 10, 20),
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::renamed_added_at_old_path_then_both_modified(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileAdded("a.txt".into(), 10),
            Event::FileModified("c.txt".into(), 1, 1),
            Event::FileModified("a.txt".into(), 10, 10),
        ],
        vec![
            SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") },
            SyncAction::Upload(path_buf("a.txt")),
        ]
    )]
    #[case::replaced_then_replaced_again(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 10,
            },
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 10,
                to: 20,
            },
        ],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::replaced_twice_then_renamed(
        vec![
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 1,
                to: 10,
            },
            Event::FileReplaced {
                path: "a.txt".into(),
                from: 10,
                to: 20,
            },
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 20,
            },
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::renamed_then_removed_then_added_with_different_path_and_inode(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRemoved("c.txt".into(), 1),
            Event::FileAdded("d.txt".into(), 1),
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("d.txt") }]
    )]
    #[case::modified_then_renamed_then_removed_then_added_same_path(
        vec![
            Event::FileModified("a.txt".into(), 1, 1),
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileRemoved("c.txt".into(), 1),
            Event::FileAdded("c.txt".into(), 1),
        ],
        vec![SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[rstest]
    #[case::created_then_modified(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileModified("c.txt".into(), 20, 20),
        ],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::created_then_modified_with_inode_change(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileModified("c.txt".into(), 20, 30),
        ],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::created_then_renamed(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 20,
            },
        ],
        vec![SyncAction::Upload(path_buf("d.txt"))]
    )]
    #[case::created_then_renamed_then_modified(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 20,
            },
            Event::FileModified("d.txt".into(), 20, 20),
        ],
        vec![SyncAction::Upload(path_buf("d.txt"))]
    )]
    #[case::created_then_removed_is_no_op(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRemoved("c.txt".into(), 20),
        ],
        vec![]
    )]
    #[case::created_then_modified_then_removed_is_no_op(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileModified("c.txt".into(), 20, 20),
            Event::FileRemoved("c.txt".into(), 20),
        ],
        vec![]
    )]
    #[case::created_then_renamed_then_removed_is_no_op(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 20,
            },
            Event::FileRemoved("d.txt".into(), 20),
        ],
        vec![]
    )]
    #[case::created_then_replaced(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileReplaced {
                path: "c.txt".into(),
                from: 20,
                to: 30,
            },
        ],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::created_then_replaced_then_renamed(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileReplaced {
                path: "c.txt".into(),
                from: 20,
                to: 30,
            },
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 30,
            },
        ],
        vec![SyncAction::Upload(path_buf("d.txt"))]
    )]
    #[case::created_then_rename_chain(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 20,
            },
            Event::FileRenamed {
                from: "d.txt".into(),
                to: "e.txt".into(),
                inode: 20,
            },
            Event::FileRenamed {
                from: "e.txt".into(),
                to: "f.txt".into(),
                inode: 20,
            },
        ],
        vec![SyncAction::Upload(path_buf("f.txt"))]
    )]
    #[case::created_then_modified_multiple_times(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileModified("c.txt".into(), 20, 20),
            Event::FileModified("c.txt".into(), 20, 20),
            Event::FileModified("c.txt".into(), 20, 20),
        ],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::created_then_removed_then_added_back(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRemoved("c.txt".into(), 20),
            Event::FileAdded("c.txt".into(), 20),
        ],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::created_then_removed_then_added_different_inode(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRemoved("c.txt".into(), 20),
            Event::FileAdded("c.txt".into(), 30),
        ],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::existing_modified_and_new_created(
        vec![
            Event::FileModified("a.txt".into(), 1, 1),
            Event::FileCreated("c.txt".into(), 20),
        ],
        vec![
            SyncAction::Upload(path_buf("a.txt")),
            SyncAction::Upload(path_buf("c.txt")),
        ]
    )]
    #[case::existing_renamed_and_new_created_at_old_path(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileCreated("a.txt".into(), 20),
        ],
        vec![
            SyncAction::Move { from: path_buf("a.txt"), to: path_buf("c.txt") },
            SyncAction::Upload(path_buf("a.txt")),
        ]
    )]
    #[case::existing_removed_and_new_created(
        vec![
            Event::FileRemoved("a.txt".into(), 1),
            Event::FileCreated("c.txt".into(), 20),
        ],
        vec![
            SyncAction::Delete(path_buf("a.txt")),
            SyncAction::Upload(path_buf("c.txt")),
        ]
    )]
    #[case::two_files_created(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileCreated("d.txt".into(), 30),
        ],
        vec![
            SyncAction::Upload(path_buf("c.txt")),
            SyncAction::Upload(path_buf("d.txt")),
        ]
    )]
    #[case::created_then_renamed_then_modified_then_replaced(
        vec![
            Event::FileCreated("c.txt".into(), 20),
            Event::FileRenamed {
                from: "c.txt".into(),
                to: "d.txt".into(),
                inode: 20,
            },
            Event::FileModified("d.txt".into(), 20, 20),
            Event::FileReplaced {
                path: "d.txt".into(),
                from: 20,
                to: 40,
            },
        ],
        vec![SyncAction::Upload(path_buf("d.txt"))]
    )]
    #[case::existing_renamed_and_new_created_then_both_modified(
        vec![
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "c.txt".into(),
                inode: 1,
            },
            Event::FileCreated("d.txt".into(), 20),
            Event::FileModified("c.txt".into(), 1, 1),
            Event::FileModified("d.txt".into(), 20, 20),
        ],
        vec![
            SyncAction::MoveAndUpload { from: path_buf("a.txt"), to: path_buf("c.txt") },
            SyncAction::Upload(path_buf("d.txt")),
        ]
    )]
    fn matrix_complex_multi_event_cases(
        #[case] events: Vec<Event>,
        #[case] expected: Vec<SyncAction>,
    ) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // A) Single-event cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_modified_same_inode(
        vec![Event::FileModified("a.txt".into(), 1, 1)],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::single_modified_different_inode(
        vec![Event::FileModified("a.txt".into(), 1, 10)],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[case::single_renamed(
        vec![Event::FileRenamed {
            from: "a.txt".into(),
            to: "c.txt".into(),
            inode: 1,
        }],
        vec![SyncAction::Move { from: path_buf("a.txt"), to: path_buf("c.txt") }]
    )]
    #[case::single_removed(
        vec![Event::FileRemoved("a.txt".into(), 1)],
        vec![SyncAction::Delete(path_buf("a.txt"))]
    )]
    #[case::single_added(
        vec![Event::FileAdded("c.txt".into(), 20)],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    #[case::single_replaced(
        vec![Event::FileReplaced {
            path: "a.txt".into(),
            from: 1,
            to: 10,
        }],
        vec![SyncAction::Upload(path_buf("a.txt"))]
    )]
    #[rstest]
    #[case::single_created(
        vec![Event::FileCreated("c.txt".into(), 20)],
        vec![SyncAction::Upload(path_buf("c.txt"))]
    )]
    fn matrix_single_event(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Folder Removed test cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::folder_removed_with_one_file(
        vec![Event::FolderRemoved("f".into(), 3)],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_in_folder_modified_then_folder_removed(
        vec![
            Event::FileModified("f/b.txt".into(), 4, 4),
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_in_folder_renamed_out_then_folder_removed(
        vec![
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "c.txt".into(),
                inode: 4,
            },
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::Move {
                from: path_buf("f/b.txt"),
                to: path_buf("c.txt"),
            },
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_in_folder_renamed_within_then_folder_removed(
        vec![
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "f/c.txt".into(),
                inode: 4,
            },
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_created_in_folder_then_folder_removed(
        vec![
            Event::FileCreated("f/new.txt".into(), 20),
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_added_in_folder_then_folder_removed(
        vec![
            Event::FileAdded("f/new.txt".into(), 20),
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_in_folder_replaced_then_folder_removed(
        vec![
            Event::FileReplaced {
                path: "f/b.txt".into(),
                from: 4,
                to: 20,
            },
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::folder_removed_then_folder_added_same_inode_file_returns(
        vec![
            Event::FolderRemoved("f".into(), 3),
            Event::FolderAdded("f".into(), 3),
            Event::FileAdded("f/b.txt".into(), 4),
        ],
        vec![
            SyncAction::Upload(path_buf("f/b.txt")),
        ]
    )]
    #[case::folder_removed_then_folder_added_different_inode_different_file(
        vec![
            Event::FolderRemoved("f".into(), 3),
            Event::FolderAdded("f".into(), 30),
            Event::FileAdded("f/new.txt".into(), 50),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
            SyncAction::Upload(path_buf("f/new.txt")),
        ]
    )]
    #[case::folder_removed_then_folder_added_same_inode_different_file(
        vec![
            Event::FolderRemoved("f".into(), 3),
            Event::FolderAdded("f".into(), 3),
            Event::FileAdded("f/new.txt".into(), 50),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::Upload(path_buf("f/new.txt")),
        ]
    )]
    #[case::file_removed_from_folder_then_folder_removed(
        vec![
            Event::FileRemoved("f/b.txt".into(), 4),
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    fn matrix_folder_removed_cases(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Folder single-event cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_folder_created(
        vec![Event::FolderCreated("g".into(), 40)],
        vec![SyncAction::EnsureFolder(path_buf("g"))]
    )]
    #[case::single_folder_added(
        vec![Event::FolderAdded("g".into(), 40)],
        vec![SyncAction::EnsureFolder(path_buf("g"))]
    )]
    #[case::single_folder_removed(
        vec![Event::FolderRemoved("f".into(), 3)],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::single_folder_renamed(
        vec![Event::FolderRenamed {
            from: "f".into(),
            to: "g".into(),
            inode: 3,
        }],
        vec![SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") }]
    )]
    #[case::single_folder_replaced(
        vec![Event::FolderReplaced {
            path: "f".into(),
            from: 3,
            to: 30,
        }],
        vec![SyncAction::Delete(path_buf("f/b.txt")), SyncAction::EnsureFolder(path_buf("f"))]
    )]
    fn matrix_folder_single_event(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Folder Added test cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::folder_added_with_file(
        vec![
            Event::FolderAdded("g".into(), 40),
            Event::FileAdded("g/x.txt".into(), 50),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::Upload(path_buf("g/x.txt")),
        ]
    )]
    #[case::folder_added_with_multiple_files(
        vec![
            Event::FolderAdded("g".into(), 40),
            Event::FileAdded("g/x.txt".into(), 50),
            Event::FileAdded("g/y.txt".into(), 51),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::Upload(path_buf("g/x.txt")),
            SyncAction::Upload(path_buf("g/y.txt")),
        ]
    )]
    #[case::folder_added_then_removed_is_noop(
        vec![
            Event::FolderAdded("g".into(), 40),
            Event::FileAdded("g/x.txt".into(), 50),
            Event::FolderRemoved("g".into(), 40),
        ],
        vec![]
    )]
    #[case::folder_added_then_renamed_then_removed_is_noop(
        vec![
            Event::FolderAdded("g".into(), 40),
            Event::FileAdded("g/x.txt".into(), 50),
            Event::FolderRenamed {
                from: "g".into(),
                to: "h".into(),
                inode: 40,
            },
            Event::FileRenamed {
                from: "g/x.txt".into(),
                to: "h/x.txt".into(),
                inode: 50,
            },
            Event::FolderRemoved("h".into(), 40),
        ],
        vec![]
    )]
    fn matrix_folder_added_cases(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Folder Created test cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::folder_created_with_file_created(
        vec![
            Event::FolderCreated("g".into(), 40),
            Event::FileCreated("g/x.txt".into(), 50),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::Upload(path_buf("g/x.txt")),
        ]
    )]
    #[case::folder_created_then_file_created_then_file_modified(
        vec![
            Event::FolderCreated("g".into(), 40),
            Event::FileCreated("g/x.txt".into(), 50),
            Event::FileModified("g/x.txt".into(), 50, 50),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::Upload(path_buf("g/x.txt")),
        ]
    )]
    #[case::folder_created_then_removed_is_noop(
        vec![
            Event::FolderCreated("g".into(), 40),
            Event::FileCreated("g/x.txt".into(), 50),
            Event::FileRemoved("g/x.txt".into(), 50),
            Event::FolderRemoved("g".into(), 40),
        ],
        vec![]
    )]
    #[case::folder_created_then_renamed(
        vec![
            Event::FolderCreated("g".into(), 40),
            Event::FolderRenamed {
                from: "g".into(),
                to: "h".into(),
                inode: 40,
            },
        ],
        vec![SyncAction::EnsureFolder(path_buf("h"))]
    )]
    fn matrix_folder_created_cases(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Folder Renamed test cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::folder_renamed_then_file_modified_inside(
        vec![
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "g/b.txt".into(),
                inode: 4
            },
            Event::FileModified("g/b.txt".into(), 4, 4),
        ],
        vec![
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::Upload(path_buf("g/b.txt")),
        ]
    )]
    #[case::folder_renamed_back_is_noop(
        vec![
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "g/b.txt".into(),
                inode: 4
            },
            Event::FolderRenamed {
                from: "g".into(),
                to: "f".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "g/b.txt".into(),
                to: "f/b.txt".into(),
                inode: 4
            },
        ],
        vec![]
    )]
    #[case::folder_renamed_then_removed(
        vec![
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "g/b.txt".into(),
                inode: 4
            },
            Event::FolderRemoved("g".into(), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::folder_rename_chain(
        vec![
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "g/b.txt".into(),
                inode: 4
            },
            Event::FolderRenamed {
                from: "g".into(),
                to: "h".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "g/b.txt".into(),
                to: "h/b.txt".into(),
                inode: 4
            },
        ],
        vec![SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("h") }]
    )]
    fn matrix_folder_renamed_cases(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Folder Replaced test cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::folder_replaced_then_files_added(
        vec![
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FileAdded("f/x.txt".into(), 50),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
            SyncAction::Upload(path_buf("f/x.txt")),
        ]
    )]
    #[case::folder_replaced_then_same_name_file_added_new_inode(
        vec![
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FileAdded("f/b.txt".into(), 40),
        ],
        vec![
            SyncAction::Upload(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
        ]
    )]
    #[case::file_remove_then_folder_replaced_with_the_removed_file(
        vec![
            Event::FileRemoved("f/b.txt".into(), 4),
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FileAdded("f/b.txt".into(), 4),
        ],
        vec![
            SyncAction::Upload(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
        ]
    )]
    #[case::folder_replaced_then_renamed(
        vec![
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 30,
            },
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
        ]
    )]
    #[case::folder_replaced_then_removed(
        vec![
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FolderRemoved("f".into(), 30),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    fn matrix_folder_replaced_cases(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Complex mixed file + folder flows
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::file_renamed_out_of_folder_new_file_added_then_folder_removed(
        vec![
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "c.txt".into(),
                inode: 4,
            },
            Event::FileAdded("f/new.txt".into(), 50),
            Event::FolderRemoved("f".into(), 3),
        ],
        vec![
            SyncAction::Move { from: path_buf("f/b.txt"), to: path_buf("c.txt") },
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::modify_file_rename_folder_modify_file_again(
        vec![
            Event::FileModified("f/b.txt".into(), 4, 4),
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "g/b.txt".into(),
                inode: 4
            },
            Event::FileModified("g/b.txt".into(), 4, 4),
        ],
        vec![
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::Upload(path_buf("g/b.txt")),
        ]
    )]
    #[case::folder_removed_folder_created_same_path_new_inode(
        vec![
            Event::FolderRemoved("f".into(), 3),
            Event::FolderCreated("f".into(), 40),
            Event::FileCreated("f/x.txt".into(), 50),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
            SyncAction::Upload(path_buf("f/x.txt")),
        ]
    )]
    #[case::existing_file_modified_and_folder_added_with_new_file(
        vec![
            Event::FileModified("a.txt".into(), 1, 1),
            Event::FolderAdded("g".into(), 40),
            Event::FileAdded("g/x.txt".into(), 50),
        ],
        vec![
            SyncAction::Upload(path_buf("a.txt")),
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::Upload(path_buf("g/x.txt")),
        ]
    )]
    #[case::existing_file_renamed_into_new_folder(
        vec![
            Event::FolderCreated("g".into(), 40),
            Event::FileRenamed {
                from: "a.txt".into(),
                to: "g/a.txt".into(),
                inode: 1,
            },
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::Move { from: path_buf("a.txt"), to: path_buf("g/a.txt") },
        ]
    )]
    #[case::folder_replaced_old_file_gone_new_files_added(
        vec![
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FileAdded("f/x.txt".into(), 50),
            Event::FileAdded("f/y.txt".into(), 51),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
            SyncAction::Upload(path_buf("f/x.txt")),
            SyncAction::Upload(path_buf("f/y.txt")),
        ]
    )]
    #[case::folder_and_file_removed(
        vec![
            Event::FolderRemoved("f".into(), 3),
            Event::FileRemoved("a.txt".into(), 1),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
            SyncAction::Delete(path_buf("a.txt")),
        ]
    )]
    #[case::file_replaced_in_folder_then_folder_renamed(
        vec![
            Event::FileReplaced {
                path: "f/b.txt".into(),
                from: 4,
                to: 40,
            },
            Event::FolderRenamed {
                from: "f".into(),
                to: "g".into(),
                inode: 3,
            },
            Event::FileRenamed {
                from: "f/b.txt".into(),
                to: "g/b.txt".into(),
                inode: 40
            },
        ],
        vec![
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::Upload(path_buf("g/b.txt")),
        ]
    )]
    #[case::folder_removed_then_added_at_different_path_same_inode(
        vec![
            Event::FolderRemoved("f".into(), 3),
            Event::FolderAdded("g".into(), 3),
            Event::FileAdded("g/b.txt".into(), 4),
        ],
        vec![
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::Upload(path_buf("g/b.txt")),
        ]
    )]
    #[case::new_folder_created_with_nested_folder(
        vec![
            Event::FolderCreated("g".into(), 40),
            Event::FolderCreated("g/sub".into(), 41),
            Event::FileCreated("g/sub/x.txt".into(), 50),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("g")),
            SyncAction::EnsureFolder(path_buf("g/sub")),
            SyncAction::Upload(path_buf("g/sub/x.txt")),
        ]
    )]
    #[case::all_files_modified_and_folder_replaced(
        vec![
            Event::FileModified("a.txt".into(), 1, 1),
            Event::FileModified("b.txt".into(), 2, 2),
            Event::FolderReplaced {
                path: "f".into(),
                from: 3,
                to: 30,
            },
            Event::FileAdded("f/b.txt".into(), 40),
        ],
        vec![
            SyncAction::Upload(path_buf("a.txt")),
            SyncAction::Upload(path_buf("b.txt")),
            SyncAction::Upload(path_buf("f/b.txt")),
            SyncAction::EnsureFolder(path_buf("f")),
        ]
    )]
    fn matrix_complex_mixed_flows(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // Nested folder scenarios
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::nested_folder_removed_removes_all_descendants(
        // Initial state: f/ contains sub/ which contains deep.txt
        // f/sub/ removed → should delete deep.txt then remove sub/
        vec![
            Event::FolderRemoved(path_buf("f/sub"), 5),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f/sub")),
        ]
    )]
    #[case::nested_folder_renamed_then_file_renamed_inside(
        // f/sub/ renamed to f/newsub/, then f/sub/deep.txt renamed to f/newsub/deep.txt
        vec![
            Event::FolderRenamed { from: path_buf("f/sub"), to: path_buf("f/newsub"), inode: 5 },
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("f/newsub/deep.txt"), inode: 6 },
            Event::FolderRenamed {from: path_buf("f/sub/deep"), to: path_buf("f/newsub/deep"), inode: 7},
            Event::FileRenamed {from: path_buf("f/sub/deep/deepest.txt"), to: path_buf("f/newsub/deep/deepest.txt"), inode: 8},
        ],
        vec![
            SyncAction::MoveFolder { from: path_buf("f/sub"), to: path_buf("f/newsub") },
        ]
    )]
    #[case::parent_folder_renamed_then_nested_folder_renamed(
        // f/ renamed to g/, then f/sub/ renamed to g/newsub/
        vec![
            Event::FolderRenamed { from: path_buf("f"), to: path_buf("g"), inode: 3 },
            Event::FolderRenamed { from: path_buf("f/sub"), to: path_buf("g/newsub"), inode: 5 },
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("g/newsub/deep.txt"), inode: 6 },
            Event::FileRenamed { from: path_buf("f/b.txt"), to: path_buf("g/b.txt"), inode: 4 },
        ],
        vec![
            SyncAction::Move { from: path_buf("f/b.txt"), to: path_buf("g/b.txt") },
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::Move { from: path_buf("f/sub/deep.txt"), to: path_buf("g/newsub/deep.txt") },
            SyncAction::MoveFolder { from: path_buf("f/sub"), to: path_buf("g/newsub") },
        ]
    )]
    #[case::parent_folder_removed_with_nested_subfolder(
        // f/ removed, which contains sub/ which contains deep.txt, plus f/b.txt
        vec![
            Event::FolderRemoved(path_buf("f"), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_modified_in_nested_folder_then_parent_removed(
        vec![
            Event::FileModified(path_buf("f/sub/deep.txt"), 6, 6),
            Event::FolderRemoved(path_buf("f"), 3),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_renamed_out_of_nested_folder_then_parent_removed(
        vec![
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("saved.txt"), inode: 6 },
            Event::FolderRemoved(path_buf("f"), 3),
        ],
        vec![
            SyncAction::Move { from: path_buf("f/sub/deep.txt"), to: path_buf("saved.txt") },
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::nested_folder_removed_then_parent_renamed(
        vec![
            Event::FolderRemoved(path_buf("f/sub"), 5),
            Event::FolderRenamed { from: path_buf("f"), to: path_buf("g"), inode: 3 },
            Event::FileRenamed { from: path_buf("f/b.txt"), to: path_buf("g/b.txt"), inode: 4 },
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f/sub")),
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },

        ]
    )]
    #[case::nested_folder_created_with_file(
        vec![
            Event::FolderCreated(path_buf("f/newsub"), 50),
            Event::FileCreated(path_buf("f/newsub/new.txt"), 51),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("f/newsub")),
            SyncAction::Upload(path_buf("f/newsub/new.txt")),
        ]
    )]
    #[case::nested_folder_created_then_removed_is_noop(
        vec![
            Event::FolderCreated(path_buf("f/newsub"), 50),
            Event::FileCreated(path_buf("f/newsub/new.txt"), 51),
            Event::FolderRemoved(path_buf("f/newsub"), 50),
        ],
        vec![]
    )]
    #[case::nested_folder_replaced_then_file_added(
        vec![
            Event::FolderReplaced { path: path_buf("f/sub"), from: 5, to: 50 },
            Event::FileAdded(path_buf("f/sub/new.txt"), 61),
        ],
        vec![
            SyncAction::Delete(path_buf("f/sub/deep.txt")),
            SyncAction::RemoveFolder(path_buf("f/sub/deep")),
            SyncAction::EnsureFolder(path_buf("f/sub")),
            SyncAction::Upload(path_buf("f/sub/new.txt")),
        ]
    )]
    #[case::nested_folder_replaced_old_file_gone_new_file_added(
        // f/sub/ replaced: deep.txt is gone, brand_new.txt added
        vec![
            Event::FolderReplaced { path: path_buf("f/sub"), from: 30, to: 60 },
            Event::FileAdded(path_buf("f/sub/brand_new.txt"), 70),
        ],
        vec![
            SyncAction::Delete(path_buf("f/sub/deep.txt")),
            SyncAction::RemoveFolder(path_buf("f/sub/deep")),
            SyncAction::EnsureFolder(path_buf("f/sub")),
            SyncAction::Upload(path_buf("f/sub/brand_new.txt")),
        ]
    )]
    #[case::nested_folder_renamed_then_removed(
        vec![
            Event::FolderRenamed { from: path_buf("f/sub"), to: path_buf("f/moved"), inode: 5 },
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("f/moved/deep.txt"), inode: 6 },
            Event::FolderRemoved(path_buf("f/moved"), 5),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f/sub")),
        ]
    )]
    #[case::deeply_nested_folder_removed(
        // f/sub/deep/ removed which contains deepest.txt
        vec![
            Event::FolderRemoved(path_buf("f/sub/deep"), 7),
        ],
        vec![
            SyncAction::RemoveFolder(path_buf("f/sub/deep")),
        ]
    )]
    #[case::parent_replaced_nested_folder_gone(
        // f/ replaced: sub/ and its contents are gone, new file added at top level
        vec![
            Event::FolderReplaced { path: path_buf("f"), from: 3, to: 70 },
            Event::FileAdded(path_buf("f/new.txt"), 71),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::RemoveFolder(path_buf("f/sub")),
            SyncAction::Upload(path_buf("f/new.txt")),
        ]
    )]
    #[case::nested_folder_added_with_files(
        vec![
            Event::FolderAdded(path_buf("f/newsub"), 80),
            Event::FileAdded(path_buf("f/newsub/x.txt"), 81),
            Event::FileAdded(path_buf("f/newsub/y.txt"), 82),
        ],
        vec![
            SyncAction::EnsureFolder(path_buf("f/newsub")),
            SyncAction::Upload(path_buf("f/newsub/x.txt")),
            SyncAction::Upload(path_buf("f/newsub/y.txt")),
        ]
    )]
    #[case::nested_folder_added_then_renamed(
        vec![
            Event::FolderAdded(path_buf("f/newsub"), 80),
            Event::FileAdded(path_buf("f/newsub/x.txt"), 81),
            Event::FolderRenamed { from: path_buf("f/newsub"), to: path_buf("f/renamed_sub"), inode: 80 },
            Event::FileRenamed { from: path_buf("f/newsub/x.txt"), to: path_buf("f/renamed_sub/x.txt"), inode: 81 },
        ],
        vec![
            SyncAction::Upload(path_buf("f/renamed_sub/x.txt")),
            SyncAction::EnsureFolder(path_buf("f/renamed_sub")),
        ]
    )]
    #[case::rename_parent_then_modify_file_in_nested(
        vec![
            Event::FolderRenamed { from: path_buf("f"), to: path_buf("g"), inode: 3 },
            Event::FileRenamed { from: path_buf("f/b.txt"), to: path_buf("g/b.txt"), inode: 4 },
            Event::FolderRenamed { from: path_buf("f/sub"), to: path_buf("g/sub"), inode: 5 },
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("g/sub/deep.txt"), inode: 6 },
            Event::FileModified(path_buf("g/sub/deep.txt"), 31, 31),
        ],
        vec![
            SyncAction::Move { from: path_buf("f/b.txt"), to: path_buf("g/b.txt") },
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::MoveAndUpload { from: path_buf("f/sub/deep.txt"), to: path_buf("g/sub/deep.txt") },
            SyncAction::MoveFolder { from: path_buf("f/sub"), to: path_buf("g/sub") },
        ]
    )]
    #[case::nested_folder_created_file_created_then_parent_removed(
        vec![
            Event::FolderCreated(path_buf("f/newsub"), 90),
            Event::FileCreated(path_buf("f/newsub/tmp.txt"), 91),
            Event::FolderRemoved(path_buf("f"), 3),
        ],
        vec![
            SyncAction::Delete(path_buf("f/b.txt")),
            SyncAction::Delete(path_buf("f/sub/deep.txt")),
            SyncAction::RemoveFolder(path_buf("f/sub")),
            SyncAction::RemoveFolder(path_buf("f")),
        ]
    )]
    #[case::file_replaced_in_nested_folder_then_parent_renamed(
        vec![
            Event::FileReplaced { path: path_buf("f/sub/deep.txt"), from: 6, to: 310 },
            Event::FolderRenamed { from: path_buf("f"), to: path_buf("g"), inode: 3 },
            Event::FileRenamed { from: path_buf("f/b.txt"), to: path_buf("g/b.txt"), inode: 4 },
            Event::FolderRenamed { from: path_buf("f/sub"), to: path_buf("g/sub"), inode: 5 },
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("g/sub/deep.txt"), inode: 310 },
        ],
        vec![
            SyncAction::Move { from: path_buf("f/b.txt"), to: path_buf("g/b.txt") },
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
            SyncAction::MoveAndUpload { from: path_buf("f/sub/deep.txt"), to: path_buf("g/sub/deep.txt") },
            SyncAction::MoveFolder { from: path_buf("f/sub"), to: path_buf("g/sub") },
        ]
    )]
    #[case::two_levels_of_nesting_all_renamed(
        vec![
            Event::FolderRenamed { from: path_buf("f"), to: path_buf("g"), inode: 3 },
            Event::FolderRenamed { from: path_buf("f/sub"), to: path_buf("g/sub"), inode: 5 },
            Event::FolderRenamed { from: path_buf("f/sub/deep"), to: path_buf("g/sub/deep"), inode: 7 },
            Event::FileRenamed { from: path_buf("f/b.txt"), to: path_buf("g/b.txt"), inode: 4 },
            Event::FileRenamed { from: path_buf("f/sub/deep.txt"), to: path_buf("g/sub/deep.txt"), inode: 6 },
            Event::FileRenamed { from: path_buf("f/sub/deep/deepest.txt"), to: path_buf("g/sub/deep/deepest.txt"), inode: 8 },
        ],
        vec![
            SyncAction::MoveFolder { from: path_buf("f"), to: path_buf("g") },
        ]
    )]
    #[case::deeply_nested_file_gets_renamed(
        vec![Event::FileRenamed {from: path_buf("f/sub/deep/deepest.txt"), to: path_buf("f/dd.txt"), inode: 8}],
        vec![SyncAction::Move { from: path_buf("f/sub/deep/deepest.txt"), to: path_buf("f/dd.txt") }]
    )]
    fn matrix_nested_folder_cases(#[case] events: Vec<Event>, #[case] expected: Vec<SyncAction>) {
        run_matrix_case_with_nested_folder_structure(events, expected);
    }
}
