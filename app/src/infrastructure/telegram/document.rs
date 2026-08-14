use teloxide::net::Download;
use teloxide::prelude::Requester;

use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedSourceDocument {
    ExceedsLimit { size: u64 },
    Content(Vec<u8>),
}

#[derive(Clone)]
pub struct TelegramDocumentLoader {
    bot: teloxide::Bot,
}

impl TelegramDocumentLoader {
    pub fn new(bot: teloxide::Bot) -> Self {
        Self { bot }
    }

    pub async fn prepare(
        &self,
        document_id: &str,
        max_bytes: u64,
    ) -> AppResult<PreparedSourceDocument> {
        let file = self
            .bot
            .get_file(teloxide::types::FileId(document_id.to_string()))
            .await?;
        let size = u64::from(file.meta.size);
        if size > max_bytes {
            return Ok(PreparedSourceDocument::ExceedsLimit { size });
        }

        let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap_or_default());
        self.bot.download_file(&file.path, &mut content).await?;
        Ok(PreparedSourceDocument::Content(content))
    }
}
