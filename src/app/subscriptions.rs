use std::time::SystemTime;
use std::{path::PathBuf, time::Duration};

use iced::Subscription;
use iced::futures::{SinkExt, StreamExt};
use iced::stream;

use crate::app::coalesce;
use crate::app::fs_index::FsIndex;
use crate::app::message::{Message, SyncMessage};

const ARCHIVE_WINDOW: Duration = Duration::from_secs(1 * 60); // 15 minutes
const MAX_ARCHIVE_WINDOW: Duration = Duration::from_secs(3600 * 2);

pub fn fs_watch_subscription(root: PathBuf) -> Subscription<Message> {
    println!("start watching");
    Subscription::run_with(root, fs_watch)
}

fn fs_watch(dir_root: &PathBuf) -> iced::futures::stream::BoxStream<'static, Message> {
    let dir_root = dir_root.clone();
    // `stream::channel` expects a closure that returns an async block (a Future)
    // whose output type is `()`. Use `move |mut output| async move { ... }`
    // and ignore the `Result` from `send` so the block returns `()`.
    stream::channel(
        100,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            // scan root directory to obtain inodes of files/folders
            let watcher = fs_watcher::AsyncWatcher::spawn(dir_root.as_path(), 0.5).await;
            let (mut w, mut events) = match watcher {
                Ok(value) => value,
                Err(e) => {
                    println!("Failed to start watcher: {e}");
                    return;
                }
            };

            let mut current_period = ARCHIVE_WINDOW;
            let mut interval = tokio::time::interval(current_period);
            interval.tick().await;
            let mut batch = vec![];

            loop {
                let fs_index = FsIndex::scan(&dir_root);
                tokio::select! {
                    event = events.next() => {
                        match event {
                            Some(Ok(event)) => {
                                batch.push(event);
                            },
                            Some(Err(e)) => {
                                println!("Stream closed due to {e:?}");
                                break;
                            }
                            None => break,
                        }
                    }
                    _ = interval.tick() => {
                        if batch.is_empty() {
                            println!("batch is empty increase archive window");
                            current_period *= 2;
                            if current_period > MAX_ARCHIVE_WINDOW {
                                current_period = MAX_ARCHIVE_WINDOW;
                            }
                            interval = tokio::time::interval(current_period);
                            interval.tick().await;
                            continue;
                        }

                        current_period = ARCHIVE_WINDOW;
                        interval = tokio::time::interval(current_period);
                        interval.tick().await;

                        let mut events_processer = coalesce::EventsTransaction::new(&fs_index);
                        for e in batch.iter() {
                            events_processer.append_event(&e);
                        }
                        let actions = events_processer.to_sync_actions();
                        batch.clear();

                        if actions.is_empty() {
                            continue;
                        }

                        if output
                            .send(Message::Sync(SyncMessage::ActionsReady(actions)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }

            let _ = w.stop().await;
        },
    )
    .boxed()
}
