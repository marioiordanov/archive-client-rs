use std::path::PathBuf;

use iced::Subscription;
use iced::futures::{SinkExt, StreamExt};
use iced::stream;

use crate::app::message::{Message};

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
            let (mut w, mut events) = fs_watcher::AsyncWatcher::spawn(dir_root.as_path()).await.unwrap();

            while let Some(event_result) = events.next().await {
                match event_result {
                    Ok(event) => {
                        println!("{event:?}");
                        let _ = output.send(Message::Test).await;
                    },
                    Err(e) => {
                        println!("Stream closed due to {e:?}");
                        break;
                    },
                }
            }
            let _ = w.stop().await;
        },
    )
    .boxed()
}
