//! Privileged process handling all kinds of gestures that may faciliate apps such as a popup dictionary.
//! Means include hooking up /dev/ inputs, and simulating user inputs.
//!

use anyhow::Ok as aok;
use anyhow::Result;
use futures::{stream::FuturesUnordered, StreamExt};

#[tokio::main]
async fn main() -> Result<()> {
    let mut streams = Vec::new();
    for (path, dev) in evdev::enumerate() {
        println!("{:?}", path);
        let ev = dev.into_event_stream()?;
        streams.push(ev);
    }
    let mut sa = futures::stream::select_all(streams);
    loop {
        let ev = sa.next().await;
        if let Some(ev) = ev {
            let ev = ev?;
            println!("{:?}", ev);
        } else {
            break;
        }
    }
    aok(())
}
