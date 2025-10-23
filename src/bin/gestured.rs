#![allow(unreachable_code)]
//! Privileged process handling all kinds of gestures that may faciliate apps such as a popup dictionary.
//! Means include hooking up /dev/ inputs, and simulating user inputs.
//!

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::set_permissions;
use std::fs::Permissions;
use std::future::Future;
use std::os::unix::fs::PermissionsExt;
use std::pin::Pin;
use std::time::Duration;

use anyhow::bail;
use anyhow::Result;
use async_bincode::tokio::AsyncBincodeStream;
use egui::Key;
use evdev::EventSummary;
use evdev::KeyCode;
use exponential_backoff::Backoff;
use futures::channel::oneshot;
use futures::stream::FuturesOrdered;
use futures::FutureExt;
use futures::SinkExt;
use futures::{stream::FuturesUnordered, StreamExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::time::Instant;
use tokio::time::Sleep;
use tracing::{debug, error, info, warn};
use wpopup::errors::wrap_noncritical;
use wpopup::proto;
use wpopup::proto::ProtoGesture;
use wpopup::proto::DEFAULT_SERVE_PATH;

use wpopup::errors::*;
use wpopup::proto::TapDist;

const RELEASE: i32 = 0;
const PRESS: i32 = 1;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    loop {
        let rx = monitor_all().await;
        warn!("monitor exited: {:?}", rx);
    }

    aok(())
}

async fn monitor_all() -> Result<()> {
    let mut streams = Vec::new();
    for (path, dev) in evdev::enumerate() {
        warn!("{:?}", path);
        let ev = dev.into_event_stream()?;
        streams.push(ev);
    }
    let req_time: HashMap<KeyCode, Duration> = HashMap::new();
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

    let mut press: BTreeMap<KeyCode, Instant> = BTreeMap::new();
    let mut release: BTreeMap<KeyCode, Instant> = BTreeMap::new();
    let mut sa = futures::stream::select_all(streams);
    let mut timers = FuturesUnordered::new();
    let mut tap_dist: BTreeMap<KeyCode, TapDist> = BTreeMap::new();
    let backoff_init = Backoff::new(1000, Duration::from_millis(50), Duration::from_secs(10));
    let mut backoff: Option<exponential_backoff::IntoIter> = None;
    let mut last_press: Option<(KeyCode, Instant)> = None;
    let mut last_key_taken_in_combo = false;

    loop {
        let brsx = brsx.clone();
        let (ev, t) = futures::select! {
            ev = sa.next() => (Some(ev), None),
            t = timers.next() =>  (None, Some(t))
        };
        if let Some(ev) = ev {
            if let Some(ev) = ev {
                let ev = ev;
                if let Ok(ev) = ev {
                    backoff = None;
                    match ev.destructure() {
                        EventSummary::Key(ke, code, ty) => {
                            if ty == PRESS {
                                let this = Instant::now();
                                let last_time_this_key = press.insert(code, this.clone());
                                timers.push(async move {
                                    sleep(Duration::from_millis(1000)).await;
                                    code
                                });
                                let key_tap =
                                    tap_dist.get_mut(&code).cloned().unwrap_or(TapDist::Initial);
                                let mut tapped = false;
                                if let Some(last) = last_time_this_key {
                                    let dist = this - last;

                                    if let Some((last_key, time)) = last_press {
                                        if code == last_key {
                                            if dist < Duration::from_millis(800) {
                                                tapped = true;
                                                tap_dist.insert(code, TapDist::First(dist));
                                                handle_taps(
                                                    &brsx,
                                                    code,
                                                    match key_tap {
                                                        TapDist::Initial => TapDist::First(dist),
                                                        TapDist::Rest(long) => {
                                                            info!("first tap after {:?}", long);
                                                            TapDist::First(dist)
                                                        }
                                                        _ => TapDist::Seq(dist),
                                                    },
                                                )
                                                .await;
                                            }
                                        }
                                    }

                                    if !tapped {
                                        tap_dist.insert(code, TapDist::Rest(dist));
                                    }
                                }
                                if let Some((last_key, time)) = last_press {
                                    let elapsed = this - time;

                                    if last_key_taken_in_combo {
                                        // Emit nothing
                                        last_key_taken_in_combo = false;
                                    } else if elapsed <= Duration::from_millis(500) {
                                        let kind = proto::Kind::Combo(last_key, code);
                                        info!(kind = ?kind, "combo");
                                        last_key_taken_in_combo = true;
                                        wrap_noncritical(
                                            brsx.send_async(ProtoGesture { kind, key: code }),
                                        )
                                        .await;
                                    }
                                }

                                match code {
                                    KeyCode::BTN_LEFT
                                    | KeyCode::BTN_RIGHT
                                    | KeyCode::KEY_SCROLLUP
                                    | KeyCode::KEY_SCROLLDOWN => {}
                                    _ => last_press = Some((code, this)),
                                }
                            }
                            if ty == RELEASE {
                                release.insert(code, Instant::now());
                            }
                        }
                        _ => {}
                    }
                } else {
                    error!(ev=?ev, "error reading dev");
                    if let Some(back) = &mut backoff {
                        let du = back.next();
                        if let Some(Some(du)) = du {
                            if du > Duration::from_secs(5) {
                                bail!("restart");
                            }
                            info!("back off for {:?}", &du);
                            sleep(du).await;
                        } else {
                            break;
                        }
                    } else {
                        backoff = Some(backoff_init.iter());
                    }
                }
            } else {
                break;
            }
        } else if let Some(t) = t {
            if let Some(t) = t {
                let rel = release.get(&t);
                if let Some(rel) = rel {
                    let delta = rel.elapsed();
                    if delta > Duration::from_millis(1000) {
                        handle_longpress(&brsx, t).await;
                    }
                }
            }
        }
    }
    aok(())
}

async fn handle_longpress(sx: &flume::Sender<ProtoGesture>, code: KeyCode) {
    info!(code = ?code, "long press");
    wrap_noncritical(sx.send_async(ProtoGesture {
        kind: proto::Kind::LongPress,
        key: code,
    }))
    .await;
}

async fn handle_taps(sx: &flume::Sender<ProtoGesture>, code: KeyCode, time: TapDist) {
    info!(code = ?code, "taps {:?}", &time);
    wrap_noncritical(sx.send_async(ProtoGesture {
        kind: proto::Kind::Taps(time),
        key: code,
    }))
    .await;
}
