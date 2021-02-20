use std::sync::Arc;

use logwatcher::{LogWatcher, LogWatcherAction};
use serenity::{async_trait, model::{channel::Message, gateway::Ready, id::ChannelId}, prelude::*};
use tokio::sync::mpsc;
use tokio::fs;

struct Handler {
    channel_id: u64,
    rx: Arc<Mutex<mpsc::Receiver<String>>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, _msg: Message) {
        //
    }

    async fn ready(&self, ctx: Context, _ready: Ready) {
        let rx = Arc::clone(&self.rx);
        let ctx_clone = Arc::new(ctx);
        let channel = ChannelId(self.channel_id);
        tokio::spawn(async move {
            while let Some(line) = rx.lock().await.recv().await {
                if let Err(e) = channel.say(&ctx_clone, line).await {
                    println!("Error sending message: {:?}", e);
                }
            }
        });
        println!("ready");
    }
}

#[derive(serde::Deserialize)]
struct Config {
    channel_id: u64,
    discord_token: String,
    log_file_path: String,
}

#[tokio::main]
async fn main() {
    let config_str = fs::read_to_string("config.toml").await.expect("Missing config.toml");
    let config: Config = toml::from_str(&config_str).expect("invalid config.toml");

    let (tx, rx) = mpsc::channel(100);

    let mut logwatcher = LogWatcher::register(config.log_file_path).expect("could not register logwatcher");
    logwatcher.watch(&mut move |line| {
        tx.blocking_send(line).unwrap();
        LogWatcherAction::None
    });

    let mut discord_client = Client::builder(&config.discord_token)
        .event_handler(Handler {
            channel_id: config.channel_id,
            rx: Arc::new(Mutex::new(rx)),
        })
        .await
        .expect("error creating client");

    if let Err(e) = discord_client.start().await {
        println!("Client error: {:?}", e);
    }
}
