//! Privileged process handling all kinds of gestures that may faciliate apps such as a popup dictionary.
//! Means include hooking up /dev/ inputs, and simulating user inputs.
//!

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Ok as aok;
use anyhow::Result;
use evdev::EventSummary;
use evdev::KeyCode;
use futures::channel::oneshot;
use futures::{stream::FuturesUnordered, StreamExt};
use layer_shell_wgpu_egui::proto;
use tracing::debug;
use tracing::info;
use tracing::warn;

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
    let mut sx_map: HashMap<KeyCode, oneshot::Sender<()>> = HashMap::new();

    let mut sa = futures::stream::select_all(streams);
    loop {
        let ev = sa.next().await;
        if let Some(ev) = ev {
            let ev = ev?;
            match ev.destructure() {
                EventSummary::Key(ke, code, ty) => {
                    debug!("{:?} {}", code, ty);
                    let sx = sx_map.remove(&code);
                    if let Some(sx) = sx {
                        if ty == RELEASE {
                            let _ = sx.send(());
                        } else {
                            sx_map.insert(code, sx);
                        }
                    } else {
                        if ty == PRESS {
                            let (sx, rx) = oneshot::channel::<()>();
                            let time = req_time
                                .get(&code)
                                .cloned()
                                .unwrap_or(Duration::from_millis(1000));
                            sx_map.insert(code, sx);
                            tokio::spawn(async move {
                                tokio::select! {
                                    _ = tokio::time::sleep(time) => {
                                        info!(code = ?code, "long press")
                                    },
                                    _ = rx => {
                                        debug!("early interrupted long press event");
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
