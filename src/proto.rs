use std::time::Duration;

pub use evdev::KeyCode;
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
    Taps(TapDist),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapDist {
    Initial,
    /// First tap after long duration of no key down
    First(Duration),
    /// Follow-up taps indicating double tap or more
    Seq(Duration),
    Rest(Duration)
}

pub const DEFAULT_SERVE_PATH: &str = "/var/run/gestured.sock";