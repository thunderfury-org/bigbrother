use crate::{
    application::ports::{CommunityCatalog, CommunityThread, CommunityThreadShares},
    error::{AppError, AppResult},
    infrastructure::client::pan1,
};

use super::parse::{parse_search_html, parse_thread_html, search_url, thread_url};

#[derive(Clone)]
pub struct Pan1CommunityCatalog {
    client: pan1::Client,
}

impl Pan1CommunityCatalog {
    pub fn new(client: pan1::Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl CommunityCatalog for Pan1CommunityCatalog {
    async fn search_threads(&self, keyword: &str, limit: u64) -> AppResult<Vec<CommunityThread>> {
        let url = search_url(self.client.base_url(), keyword);
        let html = self.client.get_html(&url).await?;
        let mut threads = parse_search_html(&html, self.client.base_url());
        if limit > 0 && threads.len() > limit as usize {
            threads.truncate(limit as usize);
        }
        Ok(threads)
    }

    async fn share_urls_for_thread(&self, tid: i64) -> AppResult<CommunityThreadShares> {
        let html = self
            .client
            .get_html(&thread_url(self.client.base_url(), tid))
            .await?;
        let mut page = parse_thread_html(&html, tid, self.client.base_url());
        if !page.logged_in {
            return Err(AppError::Unauthorized(
                "pan1.me 未登录，请在 config.yaml 填写 pan1.cookie".into(),
            ));
        }

        if page.hidden {
            self.client.reply(tid).await?;
            let html = self
                .client
                .get_html(&thread_url(self.client.base_url(), tid))
                .await?;
            page = parse_thread_html(&html, tid, self.client.base_url());
        }

        if page.hidden {
            return Err(AppError::ExternalService(
                format!("帖子 {tid} 解锁失败，仍需回复后查看"),
                false,
            ));
        }
        if page.share_urls.is_empty() {
            return Err(AppError::NotFound(format!(
                "帖子 {tid} 中没有可识别的分享链接"
            )));
        }

        Ok(CommunityThreadShares {
            tid,
            title: page.title,
            share_urls: page.share_urls,
        })
    }
}
