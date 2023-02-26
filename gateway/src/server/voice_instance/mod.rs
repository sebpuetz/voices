use async_trait::async_trait;

pub struct VoiceHost {}

impl VoiceHost {
    pub async fn open_connection() {}
}

#[async_trait]
pub trait VoiceServer {
    async fn open_connection();
}
