use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    application::{file_index::description_hash, import_ports::TitleExtractor},
    domain::media::{LANGUAGE_CHINESE, LANGUAGE_ENGLISH, LANGUAGE_JAPANESE, Title},
    error::AppResult,
    infrastructure::{
        client::openai::{ChatMessage, Client as OpenaiClient},
        entity::model::file_description,
    },
};

const SYSTEM_PROMPT: &str = "你是一个媒体信息识别助手。根据以下文本，提取出媒体（电影或电视剧）的名称和年份。\n如果文本中没有可识别的媒体信息，返回空。\n\n返回 JSON 格式：\n{\"title\": \"媒体名称\", \"year\": \"年份\", \"language\": \"语言代码\"}\n语言代码: zh（中文）/ en（英文）/ jp（日文）\n\n只返回 JSON，不要其他内容。";

#[derive(Clone)]
pub(crate) struct TitleExtractorService {
    llm_client: Option<OpenaiClient>,
    db: sea_orm::DatabaseConnection,
}

#[derive(serde::Deserialize)]
struct LlmTitleResponse {
    title: Option<String>,
    #[allow(dead_code)]
    year: Option<String>,
    language: Option<String>,
}

impl TitleExtractorService {
    pub fn new(llm_client: Option<OpenaiClient>, db: sea_orm::DatabaseConnection) -> Self {
        Self { llm_client, db }
    }
}

impl TitleExtractor for TitleExtractorService {
    async fn extract_title(&self, description: &str) -> AppResult<Option<Title>> {
        let trimmed = description.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let Some(client) = &self.llm_client else {
            return Ok(None);
        };

        let content_hash = description_hash(trimmed);

        // Check cache
        if let Some(record) = file_description::Entity::find()
            .filter(file_description::Column::ContentHash.eq(&content_hash))
            .one(&self.db)
            .await?
        {
            if let Some(title) = record.extracted_title
                && !title.is_empty()
            {
                return Ok(Some(Title {
                    title,
                    language: record.extracted_language.unwrap_or_default(),
                }));
            }
            // Record exists but no valid title — already processed, skip LLM
            return Ok(None);
        }

        // Call LLM
        let response = client
            .chat_completion(vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: SYSTEM_PROMPT.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: trimmed.to_string(),
                },
            ])
            .await?;

        let Some(content) = response else {
            write_cache(&self.db, &content_hash, None, None).await?;
            return Ok(None);
        };

        let parsed: Result<LlmTitleResponse, _> = serde_json::from_str(&content);
        let result = match parsed {
            Ok(resp) => {
                let title = resp.title.filter(|t| !t.is_empty());
                let language = resp.language.and_then(|l| normalize_language(&l));
                let title_obj = title.map(|t| Title {
                    title: t,
                    language: language.clone().unwrap_or_default(),
                });
                write_cache(
                    &self.db,
                    &content_hash,
                    title_obj.as_ref(),
                    language.as_deref(),
                )
                .await?;
                title_obj
            }
            Err(_) => {
                write_cache(&self.db, &content_hash, None, None).await?;
                None
            }
        };

        Ok(result)
    }
}

fn normalize_language(lang: &str) -> Option<String> {
    let normalized = lang.trim().to_lowercase();
    match normalized.as_str() {
        "zh" | "zh-cn" | "zh-tw" | "chinese" => Some(LANGUAGE_CHINESE.to_string()),
        "en" | "english" => Some(LANGUAGE_ENGLISH.to_string()),
        "jp" | "ja" | "japanese" => Some(LANGUAGE_JAPANESE.to_string()),
        _ => None,
    }
}

async fn write_cache(
    db: &sea_orm::DatabaseConnection,
    content_hash: &str,
    title: Option<&Title>,
    language: Option<&str>,
) -> AppResult<()> {
    if let Some(record) = file_description::Entity::find()
        .filter(file_description::Column::ContentHash.eq(content_hash))
        .one(db)
        .await?
    {
        let mut active: file_description::ActiveModel = record.into();
        active.extracted_title = ActiveValue::Set(title.map(|t| t.title.clone()));
        active.extracted_language = ActiveValue::Set(language.map(|l| l.to_string()));
        active.update(db).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_language_maps_known_codes() {
        assert_eq!(normalize_language("zh"), Some("zh".into()));
        assert_eq!(normalize_language("ZH"), Some("zh".into()));
        assert_eq!(normalize_language("chinese"), Some("zh".into()));
        assert_eq!(normalize_language("en"), Some("en".into()));
        assert_eq!(normalize_language("english"), Some("en".into()));
        assert_eq!(normalize_language("jp"), Some("jp".into()));
        assert_eq!(normalize_language("ja"), Some("jp".into()));
        assert_eq!(normalize_language("japanese"), Some("jp".into()));
    }

    #[test]
    fn normalize_language_returns_none_for_unknown() {
        assert_eq!(normalize_language("ko"), None);
        assert_eq!(normalize_language("fr"), None);
        assert_eq!(normalize_language(""), None);
    }

    #[tokio::test]
    async fn extract_title_returns_none_for_empty_description() {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        let service = TitleExtractorService::new(None, db);

        assert!(service.extract_title("").await.unwrap().is_none());
        assert!(service.extract_title("   ").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn extract_title_returns_none_when_llm_not_configured() {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        let service = TitleExtractorService::new(None, db);

        assert!(
            service
                .extract_title("民调局异闻录第三季")
                .await
                .unwrap()
                .is_none()
        );
    }
}
