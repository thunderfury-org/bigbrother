use crate::{error::AppResult, event::SendTelegramMessage};

pub trait TelegramMessagePublisher {
    async fn publish(&self, payload: &SendTelegramMessage) -> AppResult<()>;
}

pub trait TelegramMessageSender {
    async fn send(&self, payload: &SendTelegramMessage) -> AppResult<()>;
}

#[derive(Clone)]
pub struct PublishTelegramMessageService<P> {
    publisher: P,
}

impl<P> PublishTelegramMessageService<P> {
    pub fn new(publisher: P) -> Self {
        Self { publisher }
    }
}

impl<P> PublishTelegramMessageService<P>
where
    P: TelegramMessagePublisher,
{
    pub async fn send_message<T: Into<String>>(
        &self,
        text: T,
        reply_to: Option<i32>,
    ) -> AppResult<()> {
        let payload = SendTelegramMessage {
            message: text.into(),
            reply_to,
        };
        self.publisher.publish(&payload).await
    }
}

pub struct DeliverTelegramMessageService<S> {
    sender: S,
}

impl<S> DeliverTelegramMessageService<S> {
    pub fn new(sender: S) -> Self {
        Self { sender }
    }
}

impl<S> DeliverTelegramMessageService<S>
where
    S: TelegramMessageSender,
{
    pub async fn deliver(&self, payload: &SendTelegramMessage) -> AppResult<()> {
        self.sender.send(payload).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::error::AppError;

    #[derive(Clone, Default)]
    struct FakePublisher {
        payloads: Arc<Mutex<Vec<SendTelegramMessage>>>,
    }

    impl TelegramMessagePublisher for FakePublisher {
        async fn publish(&self, payload: &SendTelegramMessage) -> AppResult<()> {
            self.payloads.lock().unwrap().push(SendTelegramMessage {
                message: payload.message.clone(),
                reply_to: payload.reply_to,
            });
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeSender {
        payloads: Arc<Mutex<Vec<SendTelegramMessage>>>,
        fail: bool,
    }

    impl TelegramMessageSender for FakeSender {
        async fn send(&self, payload: &SendTelegramMessage) -> AppResult<()> {
            if self.fail {
                return Err(AppError::Internal("send failed".to_string()));
            }
            self.payloads.lock().unwrap().push(SendTelegramMessage {
                message: payload.message.clone(),
                reply_to: payload.reply_to,
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn send_text_publishes_payload() {
        let publisher = FakePublisher::default();
        let service = PublishTelegramMessageService::new(publisher.clone());

        service.send_message("hello", Some(1)).await.unwrap();

        let payloads = publisher.payloads.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].message, "hello");
        assert_eq!(payloads[0].reply_to, Some(1));
    }

    #[tokio::test]
    async fn deliver_uses_sender() {
        let sender = FakeSender::default();
        let service = DeliverTelegramMessageService::new(sender.clone());
        let payload = SendTelegramMessage {
            message: "hi".to_string(),
            reply_to: None,
        };

        service.deliver(&payload).await.unwrap();

        let payloads = sender.payloads.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].message, "hi");
    }
}
