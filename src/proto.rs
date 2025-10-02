use std::time::Duration;

use evdev::KeyCode;
use serde::{Deserialize, Serialize};
use std::time::Instant;

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
    Taps(TapDist),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapDist {
    Initial,
    First(Duration),
    Seq(Duration),
    Rest(Duration)
}

pub const DEFAULT_SERVE_PATH: &str = "/var/run/gestured.sock";