use std::env::temp_dir;
use std::rc::Rc;
use std::sync::Arc;
use std::{path::PathBuf, time::Duration};

use hyper_util::rt::TokioIo;
use iced::futures::{SinkExt, StreamExt};
use iced::stream;
use iced::{Subscription, Task};
use log::warn;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UnixListener, UnixSocket, UnixStream};

use crate::app::coalesce;
use crate::app::fs_index::FsIndex;
use crate::app::message::{Message, SyncMessage, UnixSocketCommand};

const ARCHIVE_WINDOW: Duration = Duration::from_secs(20); // 15 minutes
const MAX_ARCHIVE_WINDOW: Duration = Duration::from_secs(3600 * 2);

pub fn fs_watch_subscription(root: PathBuf) -> Subscription<Message> {
    println!("start watching");
    Subscription::run_with(root, fs_watch)
}

pub fn tcp_server_subscription() -> Subscription<Message> {
    println!("unix server start watching");
    Subscription::run(tcp_subscription)
}

pub fn tcp_subscription() -> iced::futures::stream::BoxStream<'static, Message> {
    stream::channel(
        10,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            let listener = TcpListener::bind("127.0.0.1:8787").await.unwrap();
            loop {
                match listener.accept().await {
                    Ok((mut stream, _)) => {
                        let msg_length = stream.read_u16_le().await.unwrap();
                        let mut buf = vec![0u8; msg_length as usize];

                        if stream.read_exact(&mut buf).await.ok().is_some() {
                            let msg = String::from_utf8(buf).unwrap();
                            let msg_parts: Vec<&str> = msg.split("@@").map(|s| s.trim()).collect();
                            let command = msg_parts.first().cloned();
                            match command {
                                Some("revisions") if msg_parts.last().is_some() => {
                                    let (tx, rx) = tokio::sync::oneshot::channel();
                                    let cmd = UnixSocketCommand::GetFileRevisions {
                                        path: msg_parts.last().unwrap().into(),
                                        sender: Box::new(tx),
                                    };
                                    output.send(Message::UnixSocket(cmd)).await;

                                    match rx.await {
                                        Ok(s) => {
                                            let b = serde_json::to_vec_pretty(&s).unwrap();
                                            tokio::io::AsyncWriteExt::write(&mut stream, &b)
                                                .await
                                                .unwrap();
                                        }
                                        Err(e) => {
                                            println!("{e}");
                                        }
                                    }
                                }
                                Some("download") if msg_parts.len() == 4 => {
                                    let file_id = msg_parts[1];
                                    let revision_id = msg_parts[2];
                                    let modified_time = msg_parts[3];

                                    let (tx, rx) = tokio::sync::oneshot::channel();
                                    let cmd = UnixSocketCommand::DownloadFileAtPath {
                                        file_id: file_id.into(),
                                        revision_id: revision_id.into(),
                                        modified_time: modified_time.into(),
                                        sender: Box::new(tx),
                                    };

                                    output.send(Message::UnixSocket(cmd)).await;

                                    if let Ok(path) = rx.await {
                                        let _ = tokio::io::AsyncWriteExt::write(&mut stream, path.as_bytes()).await;
                                    }
                                }
                                other @ _ => {
                                    println!("Unhandled case {other:?}");
                                }
                            };
                        } else {
                            println!("not read to end");
                        }
                    }
                    other => {
                        println!("{other:?}");
                    }
                }
            }
        },
    )
    .boxed()
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
            let watcher = fs_watcher::AsyncWatcher::spawn(
                dir_root.as_path(),
                0.5,
                &[&dir_root.join(".archived/").to_string_lossy().to_string()],
            )
            .await;
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
                            events_processer.append_event(e);
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
