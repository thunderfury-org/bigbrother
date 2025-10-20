use teloxide::{net::Download, prelude::*};

pub async fn run_bot(token: &str) {
    let bot = Bot::new(token);
    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        if let Some(text) = msg.text() {
            bot.send_message(msg.chat.id, text).await?;
        } else if let Some(doc) = msg.document() {
            let file = bot.get_file(doc.file.id.to_owned()).await?;
            let mut content = Vec::new();
            bot.download_file(&file.path, &mut content).await?;
            bot.send_message(msg.chat.id, format!("document: {}", String::from_utf8_lossy(&content)))
                .await?;
        }
        Ok(())
    })
    .await;
}
