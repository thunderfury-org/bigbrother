use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub message: String,
    pub reply_to: Option<i32>,
}

impl Message {
    pub fn new<T: Into<String>>(text: T, reply_to: Option<i32>) -> Self {
        Self {
            message: text.into(),
            reply_to,
        }
    }
}

pub trait MessageSender {
    async fn send(&self, payload: &Message) -> AppResult<()>;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::error::AppError;

    #[derive(Clone, Default)]
    struct FakeSender {
        payloads: Arc<Mutex<Vec<Message>>>,
        fail: bool,
    }

    impl MessageSender for FakeSender {
        async fn send(&self, payload: &Message) -> AppResult<()> {
            if self.fail {
                return Err(AppError::Internal("send failed".to_string()));
            }
            self.payloads.lock().unwrap().push(Message {
                message: payload.message.clone(),
                reply_to: payload.reply_to,
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn sender_receives_payload() {
        let sender = FakeSender::default();
        let payload = Message::new("hello", Some(1));

        sender.send(&payload).await.unwrap();

        let payloads = sender.payloads.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].message, "hello");
        assert_eq!(payloads[0].reply_to, Some(1));
    }

    #[tokio::test]
    async fn send_propagates_sender_error() {
        let sender = FakeSender {
            payloads: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        };
        let payload = Message::new("hi", None);

        let error = sender.send(&payload).await.unwrap_err();

        assert!(matches!(error, AppError::Internal(message) if message.contains("send failed")));
    }

    #[test]
    fn message_constructor_sets_fields() {
        let payload = Message::new("hi", None);

        assert_eq!(payload.message, "hi");
        assert_eq!(payload.reply_to, None);
    }

    #[test]
    fn message_serializes_stably() {
        let payload = Message {
            message: "hi".to_string(),
            reply_to: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(json, r#"{"message":"hi","reply_to":null}"#);

        let restored: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, payload);
    }
}
