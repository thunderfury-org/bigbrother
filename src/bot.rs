use teloxide::{net::Download, prelude::*};

pub async fn run_bot(token: &str) {
    let bot = Bot::new(token);
    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn handle_channel_post(msg: Message) -> ResponseResult<()> {
    println!("handle_channel_post");
    println!("channel post: {:#?}", msg);
    Ok(())
}

async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    println!("handle_message");
    println!("message: {:#?}", msg);
    if let Some(text) = msg.text() {
        bot.send_message(msg.chat.id, text).await?;
    } else if let Some(doc) = msg.document() {
        let file = bot.get_file(doc.file.id.to_owned()).await?;
        let mut content = Vec::new();
        bot.download_file(&file.path, &mut content).await?;
        bot.send_message(msg.chat.id, String::from_utf8_lossy(&content)).await?;
    }
    Ok(())
}
