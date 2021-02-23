use std::error::Error;

use logwatcher::{LogWatcher, LogWatcherAction};
use serenity::{
    async_trait,
    http::Http,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};
use tokio::fs;
use tokio::sync::mpsc;

struct Handler {
    listen_channel_id: u64,
    rcon_connection: Mutex<rcon::Connection>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, msg: Message) {
        if msg.channel_id == self.listen_channel_id && !msg.author.bot {
            // TODO handle empty messages with embeds, attachments, etc
            let message_text = format!("{}: {}", msg.author.name, msg.content);
            let message_text = message_text.replace('\\', "\\\\");
            let message_text = message_text.replace('\'', "\\'");
            println!("Got valid discord message, formating as: {}", message_text);
            if let Err(e) = self
                .rcon_connection
                .lock()
                .await
                .cmd(&format!(
                    "/silent-command game.print('[Discord] {}')",
                    message_text
                ))
                .await
            {
                println!("Couldn't send message to rcon: {:?}", e);
            }
        }
    }

    async fn ready(&self, _ctx: Context, _ready: Ready) {
        println!("Discord event handler ready");
    }
}

impl Handler {
    fn new(listen_channel_id: u64, rcon_connection: rcon::Connection) -> Handler {
        Handler {
            listen_channel_id,
            rcon_connection: Mutex::new(rcon_connection),
        }
    }
}

#[derive(serde::Deserialize, Clone)]
struct Config {
    channel_id: u64,
    discord_token: String,
    log_file_path: String,
    rcon_address: String,
    rcon_password: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("reading config");
    let config_str = fs::read_to_string("config.toml").await?;
    let config: Config = toml::from_str(&config_str)?;

    println!("setting up rcon client");
    let rcon = rcon::Connection::builder()
        .enable_factorio_quirks(true)
        .connect(config.rcon_address.clone(), &config.rcon_password)
        .await?;

    let (tx, mut rx) = mpsc::unbounded_channel();

    println!("setting up logwatcher");
    let config_clone = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut logwatcher = LogWatcher::register(config_clone.log_file_path)
            .expect("could not register logwatcher");
        logwatcher.watch(&mut move |line| {
            if let Some(msg) = try_get_log_chat_message(line) {
                tx.send(msg).expect("couldn't send line to mpsc channel");
            }
            LogWatcherAction::None
        });
        println!("logwatcher task exiting");
    });

    println!("setting up discord writer");
    let config_clone = config.clone();
    tokio::spawn(async move {
        let http = Http::new_with_token(&config_clone.discord_token);
        let channel = ChannelId(config_clone.channel_id);
        while let Some(line) = rx.recv().await {
            channel
                .say(&http, line)
                .await
                .expect("couldn't send message to discord");
        }
    });

    println!("setting up discord client");
    let mut discord_client = Client::builder(&config.discord_token)
        .event_handler(Handler::new(config.channel_id, rcon))
        .await?;

    println!("starting discord client");
    discord_client.start().await?;

    unreachable!()
}

fn try_get_log_chat_message(mut line: String) -> Option<String> {
    if let Some(offset) = line.find(" [CHAT] ") {
        line.replace_range(..offset + 8, "");
        if !line.starts_with("[Discord]") {
            return Some(line);
        }
    }

    None
}
