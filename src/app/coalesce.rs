//! Event coalescing: maps raw FS watcher events (within one debounce window)
//! to canonical [`SyncAction`]s according to the
//! `FS_EVENT_DRIVE_ACTION_MATRIX.md` specification.
//!
//! ## Summary of rules
//!
//! ### Files
//! | First → Last | Result |
//! |---|---|
//! | Created/Added → Removed | `NoOp` (transient file) |
//! | * → Removed | `Delete` |
//! | * → Created/Added/Modified | `Upload` |
//!
//! ### Folders
//! | First → Last | Result |
//! |---|---|
//! | Created/Added → Removed | `NoOp` (transient folder) |
//! | * → Removed | `RemoveFolder` |
//! | * → Created/Added | `EnsureFolder` |
//!
//! ### Renames
//! Rename chains are collapsed: `a→b, b→c` becomes `Delete(a) + Upload(c)`.
//! A swap-back `a→b, b→a` becomes `NoOp`.
//! Non-rename events are remapped to the final canonical path before
//! lifecycle reduction, so `Renamed{a→b} + Modified(b)` collapses
//! correctly to `Delete(a) + Upload(b)`.

use std::{
    collections::HashMap,
    path::{ PathBuf},
};

use crate::app::message::SyncAction;

#[derive(Clone)]
enum Action<'a> {
    Rename { to: &'a String },
    Modify,
    Remove,
}

pub(crate) struct EventsHandler<'a> {
    map: HashMap<&'a String, Action<'a>>,
    // from string to the entry in map
    reverse_link: HashMap<&'a String, &'a String>,
}

impl<'a> EventsHandler<'a> {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            reverse_link: HashMap::new(),
        }
    }

    pub(crate) fn process(mut self, events: &'a [fs_watcher::Event]) -> Vec<SyncAction> {
        for ev in events.iter() {
            match ev {
                fs_watcher::Event::Renamed { from, to } => self.rename(from, to),
                fs_watcher::Event::FileAdded(filename)
                | fs_watcher::Event::FileCreated(filename)
                | fs_watcher::Event::FileModified(filename) => {
                    self.map.insert(filename, Action::Modify);
                }
                fs_watcher::Event::FileRemoved(filename) => {
                    self.map.insert(filename, Action::Remove);
                }
                fs_watcher::Event::FolderAdded(_)
                | fs_watcher::Event::FolderCreated(_)
                | fs_watcher::Event::FolderRemoved(_) => {}
            }
        }
        let mut out = Vec::new();

        for (path, action) in self.map.iter() {
            match action {
                Action::Rename { to } => {
                    match self.map.get(to) {
                        Some(Action::Modify) => out.push(SyncAction::MoveAndUpload { from: PathBuf::from(path), to: PathBuf::from(to) }),
                        Some(Action::Remove) => out.push(SyncAction::Delete(PathBuf::from(path))),
                        _ => out.push(SyncAction::Move { from: PathBuf::from(path), to: PathBuf::from(to) }),
                    }
                }
                Action::Modify => {
                    // Skip modify if this path is a rename destination;
                    // it is already covered by MoveAndUpsert.
                    if !self.reverse_link.contains_key(*path) {
                        out.push(SyncAction::Upload(PathBuf::from(path)));
                    }
                }
                Action::Remove => {
                    if !self.reverse_link.contains_key(*path) {
                        out.push(SyncAction::Delete(PathBuf::from(path)));
                    }
                }
            }
        }

        out
    }

    // a->b , b->a
    // b->c
    //
    fn rename(&mut self, from: &'a String, to: &'a String) {
        let from_entry = self.map.get(from);
        let link_to_from_option = self.reverse_link.get(from).copied();

        match (from_entry, link_to_from_option) {
            (None, None) => {
                self.map.insert(from, Action::Rename { to });
                self.reverse_link.insert(to, from);
            }
            (None, Some(reverse_link)) if reverse_link == to => {
                // cycle, remove
                self.map.remove(to);
                self.reverse_link.remove(from);
            }
            // collapse path
            (None, Some(link_to_from)) => {
                if let Some(Action::Rename { to: forward_link }) = self.map.get_mut(link_to_from) {
                    self.reverse_link.remove(from);
                    *forward_link = to;
                    self.reverse_link.insert(to, link_to_from);
                }
            }
            (Some(action), Some(link_to_from)) => {
                let action = action.clone();
                self.map.insert(to, action);
                self.map.remove(from);
                self.reverse_link.remove(from);

                if let Some(Action::Rename { to: forward_link }) = self.map.get_mut(link_to_from) {
                    *forward_link = to;
                    self.reverse_link.insert(to, link_to_from);
                }
            }
            _ => todo!(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use fs_watcher::Event;
    use rstest::rstest;

    use super::EventsHandler;
    use crate::app::message::SyncAction;

    #[derive(Clone, Copy, Debug)]
    enum ExpectedAction {
        Upload(&'static str),
        Delete(&'static str),
        Move(&'static str, &'static str),
        MoveAndUpload(&'static str, &'static str),
    }

    fn expected_actions(expected: Vec<ExpectedAction>) -> Vec<SyncAction> {
        expected
            .into_iter()
            .map(|action| match action {
                ExpectedAction::Upload(path) => SyncAction::Upload(PathBuf::from(path)),
                ExpectedAction::Delete(path) => SyncAction::Delete(PathBuf::from(path)),
                ExpectedAction::Move(from, to) => SyncAction::Move {
                    from: PathBuf::from(from),
                    to: PathBuf::from(to),
                },
                ExpectedAction::MoveAndUpload(from, to) => SyncAction::MoveAndUpload {
                    from: PathBuf::from(from),
                    to: PathBuf::from(to),
                },
            })
            .collect()
    }

    fn action_key(action: &SyncAction) -> (String, String) {
        match action {
            SyncAction::Upload(path) => ("upload".to_string(), path.display().to_string()),
            SyncAction::Delete(path) => ("delete".to_string(), path.display().to_string()),
            SyncAction::MoveAndUpload { from, to } => (
                "move_and_upload".to_string(),
                format!("{}->{}", from.display(), to.display()),
            ),
            SyncAction::Move { from, to } => (
                "move".to_string(),
                format!("{}->{}", from.display(), to.display()),
            ),
            SyncAction::EnsureFolder(path) => {
                ("ensure_folder".to_string(), path.display().to_string())
            }
            SyncAction::RemoveFolder(path) => {
                ("remove_folder".to_string(), path.display().to_string())
            }
        }
    }

    fn assert_actions_eq_unordered(mut actual: Vec<SyncAction>, mut expected: Vec<SyncAction>) {
        actual.sort_by_key(action_key);
        expected.sort_by_key(action_key);
        let actual_keys: Vec<(String, String)> = actual.iter().map(action_key).collect();
        let expected_keys: Vec<(String, String)> = expected.iter().map(action_key).collect();
        assert_eq!(actual_keys, expected_keys);
    }

    fn run_matrix_case(events: Vec<Event>, expected: Vec<ExpectedAction>) {
        let actions = EventsHandler::new().process(&events);

        assert_actions_eq_unordered(actions, expected_actions(expected));
    }

    // -----------------------------------------------------------------------
    // A) Single-event cases
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::file_created(
        vec![Event::FileCreated("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::file_added(
        vec![Event::FileAdded("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::file_modified(
        vec![Event::FileModified("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::file_removed(
        vec![Event::FileRemoved("a.txt".into())],
        vec![ExpectedAction::Delete("a.txt")]
    )]
    #[case::rename_file(
        vec![Event::Renamed {
            from: "old.txt".into(),
            to: "new.txt".into(),
        }],
        vec![ExpectedAction::Move("old.txt", "new.txt")]
    )]
    fn matrix_single_event_cases(
        #[case] events: Vec<Event>,
        #[case] expected: Vec<ExpectedAction>,
    ) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // B) Same-path file bursts
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::created_then_modified(
        vec![Event::FileCreated("a.txt".into()), Event::FileModified("a.txt".into()), Event::FileModified("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::added_then_modified(
        vec![Event::FileAdded("a.txt".into()), Event::FileModified("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::modified_burst(
        vec![Event::FileModified("a.txt".into()), Event::FileModified("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::modified_then_removed(
        vec![Event::FileModified("a.txt".into()), Event::FileRemoved("a.txt".into())],
        vec![ExpectedAction::Delete("a.txt")]
    )]
    #[case::removed_then_created(
        vec![Event::FileRemoved("a.txt".into()), Event::FileCreated("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::removed_then_added(
        vec![Event::FileRemoved("a.txt".into()), Event::FileAdded("a.txt".into())],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    #[case::removed_created_modified(
        vec![
            Event::FileRemoved("a.txt".into()),
            Event::FileCreated("a.txt".into()),
            Event::FileModified("a.txt".into()),
        ],
        vec![ExpectedAction::Upload("a.txt")]
    )]
    fn matrix_file_burst_cases(#[case] events: Vec<Event>, #[case] expected: Vec<ExpectedAction>) {
        run_matrix_case(events, expected);
    }

    // -----------------------------------------------------------------------
    // D) Rename-centric bursts
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::rename_then_modify(
        vec![
            Event::Renamed {
                from: "a.txt".into(),
                to: "b.txt".into(),
            },
            Event::FileModified("b.txt".into()),
        ],
        vec![ExpectedAction::MoveAndUpload("a.txt", "b.txt")]
    )]
    #[case::rename_then_removed_destination(
        vec![
            Event::Renamed {
                from: "a.txt".into(),
                to: "b.txt".into(),
            },
            Event::FileRemoved("b.txt".into()),
        ],
        vec![ExpectedAction::Delete("a.txt")]
    )]
    #[case::rename_chain(
        vec![
            Event::Renamed {
                from: "a.txt".into(),
                to: "b.txt".into(),
            },
            Event::Renamed {
                from: "b.txt".into(),
                to: "c.txt".into(),
            },
        ],
        vec![ExpectedAction::Move("a.txt", "c.txt")]
    )]
    #[case::rename_chain_then_modify_terminal(
        vec![
            Event::Renamed {
                from: "a.txt".into(),
                to: "b.txt".into(),
            },
            Event::Renamed {
                from: "b.txt".into(),
                to: "c.txt".into(),
            },
            Event::FileModified("c.txt".into()),
        ],
        vec![ExpectedAction::MoveAndUpload("a.txt", "c.txt")]
    )]
    #[case::rename_swap_back(
        vec![
            Event::Renamed {
                from: "a.txt".into(),
                to: "b.txt".into(),
            },
            Event::Renamed {
                from: "b.txt".into(),
                to: "a.txt".into(),
            },
        ],
        vec![]
    )]
    fn matrix_rename_burst_cases(
        #[case] events: Vec<Event>,
        #[case] expected: Vec<ExpectedAction>,
    ) {
        run_matrix_case(events, expected);
    }
}
