#![allow(unreachable_code)]
//! Privileged process handling all kinds of gestures that may faciliate apps such as a popup dictionary.
//! Means include hooking up /dev/ inputs, and simulating user inputs.
//!

use std::collections::HashMap;
use std::fs::set_permissions;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use async_bincode::tokio::AsyncBincodeStream;
use evdev::EventSummary;
use evdev::KeyCode;
use futures::channel::oneshot;
use futures::SinkExt;
use futures::{stream::FuturesUnordered, StreamExt};
use wpopup::errors::wrap_noncritical;
use wpopup::proto;
use wpopup::proto::ProtoGesture;
use wpopup::proto::DEFAULT_SERVE_PATH;
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

use wpopup::errors::*;

const RELEASE: i32 = 0;
const PRESS: i32 = 1;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let mut streams = Vec::new();
    for (path, dev) in evdev::enumerate() {
        warn!("{:?}", path);
        let ev = dev.into_event_stream()?;
        streams.push(ev);
    }
    let req_time: HashMap<KeyCode, Duration> = HashMap::new();
    let mut sx_map: HashMap<KeyCode, oneshot::Sender<_>> = HashMap::new();
    if streams.is_empty() {
        error!("no input device found. check permissions");
        return aok(());
    }

    let sock_path = DEFAULT_SERVE_PATH;
    warn!("bind socket at {}", &sock_path);
    let _ = std::fs::remove_file(sock_path);
    let sock = UnixListener::bind(sock_path)?;
    set_permissions(sock_path, PermissionsExt::from_mode(0o777))?;
    let (brsx, rx) = flume::unbounded::<proto::ProtoGesture>();

    tokio::spawn(async move {
        loop {
            let (incom, addr) = sock.accept().await?;
            warn!("incoming client at {:?}", addr);
            let mut fm: AsyncBincodeStream<
                tokio::net::UnixStream,
                ProtoGesture,
                ProtoGesture,
                async_bincode::AsyncDestination,
            > = AsyncBincodeStream::from(incom).for_async();
            let rx = rx.clone();
            tokio::spawn(async move {
                loop {
                    let k = rx.recv_async().await?;
                    fm.send(k).await?;
                }
                aok(())
            });
        }

        aok(())
    });

    let mut sa = futures::stream::select_all(streams);
    loop {
        let brsx = brsx.clone();
        let ev = sa.next().await;
        if let Some(ev) = ev {
            let ev = ev?;
            match ev.destructure() {
                EventSummary::Key(ke, code, ty) => {
                    debug!("{:?} {}", code, ty);
                    let sx = sx_map.remove(&code);
                    if let Some(sx) = sx {
                        if ty == RELEASE {
                            let _ = sx.send(ty);
                        } else {
                            sx_map.insert(code, sx);
                        }
                    } else {
                        if ty == PRESS {
                            let (sx, rx) = oneshot::channel::<_>();
                            let time = req_time
                                .get(&code)
                                .cloned()
                                .unwrap_or(Duration::from_millis(1000));
                            sx_map.insert(code, sx);
                            tokio::spawn(async move {
                                let start = Instant::now();

                                tokio::select! {
                                    _ = tokio::time::sleep(time) => {
                                        info!(code = ?code, "long press");
                                        handle_longpress(brsx, code).await;
                                    },
                                    tap = rx => {
                                        let inter = Instant::now() - start;
                                        match tap {
                                            Ok(tap) => {
                                                if tap == PRESS {
                                                    handle_taps(brsx, code, inter).await;
                                                } else {
                                                    debug!("long press cancelled");
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                };
                            });
                        }
                    }
                }
                _ => {}
            }
        } else {
            break;
        }
    }

    aok(())
}

async fn handle_longpress(sx: flume::Sender<ProtoGesture>, code: KeyCode) {
    wrap_noncritical(sx.send_async(ProtoGesture {
        kind: proto::Kind::LongPress,
        key: code,
    }))
    .await;
}

async fn handle_taps(sx: flume::Sender<ProtoGesture>, code: KeyCode, time: Duration) {
    wrap_noncritical(sx.send_async(ProtoGesture {
        kind: proto::Kind::Taps(time),
        key: code,
    }))
    .await;
}
