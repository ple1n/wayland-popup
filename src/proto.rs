use std::time::Duration;

use evdev::KeyCode;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProtoGesture {
    pub kind: Kind,
    pub key: KeyCode,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Kind {
    Press,
    Release,
    LongPress,
    Taps(Duration)
}

pub const DEFAULT_SERVE_PATH: &str = "/var/run/gestured.sock";