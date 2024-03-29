use clap::Parser;
use uuid::Uuid;

#[derive(Parser)]
#[clap(name = "voice-client")]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    #[clap(long, default_value = "ws://localhost:33332")]
    pub ws_endpoint: String,
    #[clap(long, default_value = "http://localhost:33332")]
    pub gateway_url: String,
    #[clap(long)]
    pub client_id: Option<Uuid>,
    #[clap(long, default_value = "Foo")]
    pub name: String,
    #[clap(long)]
    pub deaf: bool,
    #[clap(long)]
    pub mute: bool,
}
