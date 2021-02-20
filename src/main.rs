use logwatcher::{LogWatcher, LogWatcherAction};
use serenity::{
    async_trait,
    http::Http,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};
use tokio::fs;
use tokio::sync::mpsc;

struct Handler {}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, _msg: Message) {
        //
    }

    async fn ready(&self, _ctx: Context, _ready: Ready) {
        println!("handler ready");
    }
}

#[derive(serde::Deserialize, Clone)]
struct Config {
    channel_id: u64,
    discord_token: String,
    log_file_path: String,
}

#[tokio::main]
async fn main() {
    println!("reading config");
    let config_str = fs::read_to_string("config.toml")
        .await
        .expect("Missing config.toml");
    let config: Config = toml::from_str(&config_str).expect("invalid config.toml");

    let (tx, mut rx) = mpsc::unbounded_channel();

    println!("setting up logwatcher");
    let config_clone = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut logwatcher = LogWatcher::register(config_clone.log_file_path)
            .expect("could not register logwatcher");
        logwatcher.watch(&mut move |mut line| {
            if let Some(offset) = line.find(" [CHAT] ") {
                line.replace_range(..offset, "");
                tx.send(line).expect("couldn't send line to mpsc channel");
            }
            LogWatcherAction::None
        });
        println!("logwatcher task exiting");
    });

    println!("setting up writer");
    let config_clone = config.clone();
    tokio::spawn(async move {
        let http = Http::new_with_token(&config_clone.discord_token);
        let channel = ChannelId(config_clone.channel_id);
        while let Some(line) = rx.recv().await {
            channel
                .say(&http, line)
                .await
                .expect("writer coudn't send message");
        }
    });

    println!("setting up discord client");
    let mut discord_client = Client::builder(&config.discord_token)
        .event_handler(Handler {})
        .await
        .expect("error creating client");

    println!("starting discord client");
    if let Err(e) = discord_client.start().await {
        println!("Client error: {:?}", e);
    }
}
