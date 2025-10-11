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
    Combo(KeyCode, KeyCode),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapDist {
    Initial,
    /// First tap after long duration of no key down
    First(Duration),
    /// Follow-up taps indicating double tap or more
    Seq(Duration),
    Rest(Duration),
}

pub const DEFAULT_SERVE_PATH: &str = "/var/run/gestured.sock";

impl ProtoGesture {
    pub fn elapsed(&self) -> Option<Duration> {
        match &self.kind {
            Kind::Taps(t) => match t {
                TapDist::First(t) => Some(*t),
                TapDist::Seq(t) => Some(*t),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn is_unordered(&self, key1: KeyCode, key2: KeyCode) -> bool {
        match self.kind {
            Kind::Combo(a, b) => (a == key1 && b == key2) || (a == key2 && b == key1),
            _ => false,
        }
    }
}
