use evdev::KeyCode;

pub struct ProtoGesture {
    kind: Kind,
    key: KeyCode,
}

#[derive(Debug)]
pub enum Kind {
    Press,
    Release,
    LongPress,
}
