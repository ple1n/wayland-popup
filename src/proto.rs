use evdev::KeyCode;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProtoGesture {
    kind: Kind,
    key: KeyCode,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Kind {
    Press,
    Release,
    LongPress,
}

pub const DEFAULT_SERVE_PATH: &str = "/var/run/gestured.sock";