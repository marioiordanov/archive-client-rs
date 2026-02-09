use std::path::PathBuf;

use iced::Subscription;
use iced::futures::{SinkExt, StreamExt};
use iced::stream;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::app::message::{Message, SyncMessage};

pub fn fs_watch_subscription(root: PathBuf) -> Subscription<Message> {
    println!("fs_watch_subscription");
    Subscription::run_with(root, fs_watch_stream)
}

fn fs_watch_stream(root: &PathBuf) -> iced::futures::stream::BoxStream<'static, Message> {
    let root = root.clone();

    stream::channel(
        100,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            let (tx, mut rx) = iced::futures::channel::mpsc::unbounded::<PathBuf>();

            let mut watcher = match RecommendedWatcher::new(
                move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => {
                                for path in event.paths {
                                    let _ = tx.unbounded_send(path);
                                }
                            }
                            _ => {}
                        }
                    }
                },
                notify::Config::default(),
            ) {
                Ok(w) => w,
                Err(_) => return,
            };

            if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
                return;
            }

            while let Some(path) = rx.next().await {
                let _ = output.send(Message::Sync(SyncMessage::FsChanged(path))).await;
            }
        },
    )
    .boxed()
}
