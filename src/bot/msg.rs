use teloxide::prelude::*;

use crate::state::AppState;

pub(super) struct MsgProcessor<'a> {
    pub state: &'a AppState,
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub matched_filter: Option<&'a str>,
}

impl MsgProcessor<'_> {
    pub(super) async fn process(&self) -> ResponseResult<()> {
        if let Some(text) = self.msg.text() {
            if text == "/start" {
                self.bot
                    .send_message(
                        get_chat_id(&state),
                        "欢迎使用秒传链接转存机器人！\n\
                 请发送包含秒传链接的 JSON 文件，我将帮助您将文件转存到您的 pan123 账号中。",
                    )
                    .await?;
            }
        } else if let Some(doc) = self.msg.document() {
            handle_document(state, bot, doc, &msg).await?;
        } else {
            let urls = get_urls_from_msg(&msg);
            if let Some(url) = urls.first() {
                handle_share_url(state, bot, url, &msg).await?;
            }
        }

        Ok(())
    }
}
