use serde::{Deserialize, Serialize};

/// multi-purpose/generic event type that can easily be used over the event streaming
#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    _type: String,
    message: String,
}

impl Event {
    pub fn new(event_type: String, message: String) -> Self {
        Self {
            _type: event_type,
            message,
        }
    }

    pub fn event_type(&self) -> &str { &self._type }

    pub fn message(&self) -> &str { &self.message }
}

pub mod controller;
