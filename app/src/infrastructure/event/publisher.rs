use crate::{
    application::notify::TelegramMessagePublisher, error::AppResult,
    infrastructure::{event::SendTelegramMessage, event_bus::EventBus},
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

impl TelegramMessagePublisher for EventBusPublisher {
    async fn publish(&self, payload: &SendTelegramMessage) -> AppResult<()> {
        self.bus.publish(payload).await
    }
}
