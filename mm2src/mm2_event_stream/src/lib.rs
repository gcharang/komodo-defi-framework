use serde::Deserialize;

/// multi-purpose/generic event type that can easily be used over the event streaming
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

/// Configuration for event streaming
#[derive(Deserialize)]
pub struct EventStreamConfiguration {
    #[serde(default)]
    pub access_control_allow_origin: String,
    #[serde(default)]
    pub active_events: Vec<EventStatus>,
}

#[derive(Clone, Default, Deserialize)]
pub struct EventStatus {
    name: String,
    pub stream_interval_seconds: f64,
}

impl Default for EventStreamConfiguration {
    fn default() -> Self {
        Self {
            access_control_allow_origin: String::from("*"),
            active_events: vec![],
        }
    }
}

impl EventStreamConfiguration {
    pub fn get_event(&self, event_name: &str) -> Option<EventStatus> {
        self.active_events
            .iter()
            .find(|event| event.name == event_name)
            .cloned()
    }
}

pub mod behaviour;
pub mod controller;
