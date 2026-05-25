use crate::{
    application::notify::{Message, MessageSender},
    error::AppResult,
    infrastructure::event_bus::EventBus,
};

#[derive(Clone)]
pub struct EventBusPublisher {
    bus: EventBus,
}

impl EventBusPublisher {
    pub fn new(bus: EventBus) -> Self {
        Self { bus }
    }
}

impl MessageSender for EventBusPublisher {
    async fn send(&self, payload: &Message) -> AppResult<()> {
        self.bus.publish(payload).await
    }
}
