use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures_util::{Future, FutureExt};
use tokio::select;
use tracing::Instrument;
use uuid::Uuid;
use voices_voice_models::ConnectionState;
use voices_ws_proto::{ClientEvent, Init, Join, Present};

use crate::channel_registry::GetVoiceHost;
use crate::server::channels::{ChannelEvent, ChannelEventKind, Channels, ClientInfo};
use crate::server::ws::ControlStream;
use crate::voice_instance::{EstablishSession, OpenConnection, VoiceHost, VoiceHostError};

use super::channels::{state::ChannelState, ChatRoomJoined};
use super::ws::ControlStreamError;

/// Representation of a client session
///
/// Owns a handle to a [`VoiceServer`] and the known [`Channels`].
///
/// Has a [`VoiceHandle`] if the client is currently in a channel.
pub struct ServerSession<R, S> {
    // client info, received in init msg
    client_id: Uuid,
    client_name: String,
    // eventloop / backwards chan
    ctl: ControlStream,
    // optionally connected channel
    voice: Option<VoiceHandle>,

    // channel registry
    channels: Channels<S, R>,
}

impl<R, S> ServerSession<R, S>
where
    R: GetVoiceHost,
    S: ChannelState,
{
    /// Initialize the session on the incoming stream.
    ///
    /// Waits for the client handshake.
    #[tracing::instrument(skip_all)]
    pub async fn init(
        ctl: axum::extract::ws::WebSocket,
        channels: Channels<S, R>,
    ) -> anyhow::Result<Self> {
        let mut ctl = ControlStream::new(ctl);
        tracing::debug!("waiting for handshake");
        let Init { user_id, name } = ctl.await_handshake().await?;
        tracing::debug!("handshake succesful: {}", user_id);
        Ok(Self {
            ctl,
            client_id: user_id,
            client_name: name,
            channels,
            voice: None,
        })
    }

    /// Run the client session to completion.
    ///
    /// Tracks the status of the optional voice connection and related channel events as well as client events.
    #[tracing::instrument(skip(self), fields(client_id=%self.client_id, name=&self.client_name))]
    pub async fn run_session(mut self) -> anyhow::Result<()> {
        tracing::info!("session running");
        loop {
            let evt = self.ctl.next_event();
            let (handle, chan_notify) = match self.voice.as_mut() {
                Some(v) => ((&mut v.status_handle).boxed(), v.room_handle.recv().boxed()),
                None => (
                    std::future::pending().boxed(),
                    std::future::pending().boxed(),
                ),
            };
            select! {
                handle = handle => {
                    if let Err(e) = handle {
                        tracing::error!("Voice errored: {}", e);
                    }
                    self.ctl.disconnected().await?;
                    self.voice = None;
                }
                evt = evt => {
                    if let Some(evt) = evt {
                        self.handle_event(evt).await?;
                    } else {
                        self.stop_voice().await;
                        return Ok(());
                    }
                },
                Ok(chan_event) = chan_notify => {
                    self.handle_chan_event(chan_event).await?;
                },
            }
        }
    }

    async fn handle_chan_event(
        &mut self,
        chan_event: ChannelEvent,
    ) -> Result<(), ControlStreamError> {
        if self.client_id == chan_event.source() {
            return Ok(());
        }
        let source_id = chan_event.source_id();
        match chan_event.kind() {
            ChannelEventKind::Joined { name: joined } => {
                self.ctl.joined(joined.into(), source_id).await?;
            }
            ChannelEventKind::Left(left) => {
                self.ctl.left(left.into(), source_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_event(&mut self, evt: ClientEvent) -> anyhow::Result<()> {
        match evt {
            ClientEvent::Init(_) => {
                tracing::warn!("unexpected init message");
                self.stop_voice().await;
                return Ok(());
            }
            ClientEvent::Disconnect => {
                self.stop_voice().await;
                return Ok(());
            }
            ClientEvent::Join(Join { channel_id }) => {
                tracing::info!("joining {}", channel_id);
                self.stop_voice().await;

                if let Some(voice) = self.initialize_voice_connection(channel_id).await? {
                    self.voice = Some(voice);
                } else {
                    self.ctl.join_error(channel_id).await?;
                }
            }
            ClientEvent::Leave => {
                tracing::info!("leave");
                self.stop_voice().await;
            }
            ClientEvent::UdpAnnounce(_) => {
                tracing::warn!("unexpected udp announce message");
                self.stop_voice().await;
            }
            ClientEvent::Keepalive(_) => {
                unreachable!("Keepalive is handled by WS task");
            }
        }
        Ok(())
    }

    /// Sends a stop signal to the voice connection if there is one.
    async fn stop_voice(&mut self) {
        if self.voice.take().is_some() {
            let _ = self.ctl.disconnected().await;
        }
    }

    /// Initialize a voice connection for the client on the requested [`Chatroom`]
    ///
    /// 1. Talks to the [`VoiceServer`] to obtain the server side voice connection info.
    /// 2. Announces this info to the client
    /// 2.1. (optional) client can use this info to discover its external IP at the voice server
    /// 3. Wait for the client to provide its external address
    /// 4. Provide client info to voice server to let it initialize the connection.
    pub async fn initialize_voice_connection(
        &mut self,
        channel_id: Uuid,
    ) -> anyhow::Result<Option<VoiceHandle>> {
        let mut channel = match self.channels.get_or_init(channel_id).await? {
            Some(channel) => channel,
            None => {
                tracing::info!("channel not found");
                return Ok(None);
            }
        };
        tracing::debug!("announcing udp");
        // should return source_id
        let client_id = self.client_id;
        let req = OpenConnection {
            user_name: self.client_name.clone(),
            channel_id,
            client_id: self.client_id,
        };
        // FIXME: forbid joining twice, could be solved with server side session ids - messes up some internal state
        let resp = match channel
            .voice()
            .open_connection(req.clone())
            .await
            .map_err(|e| {
                tracing::warn!("open connection failed: {}", e);
                e
            }) {
            Ok(o) => o,
            Err(VoiceHostError::NotFound(_)) => {
                tracing::info!("attempting to reassign voice channel");
                channel = self.channels.reassign_voice(channel_id).await?;
                channel.voice().open_connection(req).await?
            }
            Err(VoiceHostError::Other(o)) => {
                tracing::error!("aborting voice initialization");
                return Err(o);
            }
        };
        let udp_addr = resp.sock;
        tracing::debug!("udp running on {:?}", udp_addr);
        let source_id = resp.source_id;

        let info = ClientInfo {
            client_id,
            source_id,
            name: self.client_name.clone(),
        };
        let room_handle = channel.join(info).await?;
        let present = channel.list_members().await?;

        self.ctl.announce_udp(udp_addr, source_id).await?;
        tracing::debug!("announced udp, waiting for client udp addr");

        let client_udp = self.ctl.await_client_udp().await?;
        let client_udp = SocketAddr::from((client_udp.ip, client_udp.port));
        anyhow::ensure!(
            !client_udp.ip().is_unspecified(),
            "client provided unspecified IP addr: {}",
            client_udp.ip()
        );
        tracing::debug!("received client udp addr {client_udp}");
        let request = EstablishSession {
            channel_id,
            client_id,
            client_addr: client_udp,
        };
        let resp = match channel
            .voice()
            .establish_session(request)
            .await
            .map_err(|e| {
                tracing::warn!("failed to establish session: {}", e);
                e
            }) {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("aborting voice init");
                return Err(e.into());
            }
        };

        let crypt_key = base64::engine::general_purpose::STANDARD
            .encode(resp.crypt_key)
            .into();
        tracing::debug!("established voice session");

        let present = present
            .into_iter()
            .filter_map(|v| {
                (v.client_id != client_id).then_some(Present {
                    user: v.name,
                    source_id: v.source_id,
                })
            })
            .collect();

        let ready = voices_ws_proto::Ready {
            id: channel_id,
            src_id: source_id,
            present: dbg!(present),
            crypt_key,
        };
        self.ctl.voice_ready(ready).await?;
        let voice_handle = channel.voice().clone();
        let status_handle = status_check(voice_handle.clone(), client_id, channel_id);

        Ok(Some(VoiceHandle {
            client_id,
            channel_id,
            status_handle,
            voice_host: voice_handle,
            room_handle,
            source_id,
        }))
    }
}

fn status_check(
    voice_handle: Arc<dyn VoiceHost>,
    client_id: Uuid,
    channel_id: Uuid,
) -> tokio::task::JoinHandle<()> {
    // FIXME: check whether this is necessary
    tokio::spawn(
        async move {
            let mut inter = tokio::time::interval(Duration::from_secs(5));
            inter.tick().await;
            loop {
                inter.tick().await;
                match voice_handle
                    .user_status(channel_id, client_id)
                    .await
                    .map_err(|e| {
                        tracing::warn!("user status check failed: {}", e);
                        e
                    }) {
                    Ok(ConnectionState::Peered) => {
                        tracing::trace!("voice connection is in peered state");
                        continue;
                    }
                    Ok(ConnectionState::Waiting) => {
                        tracing::trace!("voice connection is in waiting state");
                        continue;
                    }
                    Ok(ConnectionState::Stopped) => {
                        tracing::info!("voice connection stopped");
                        break;
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        }
        .instrument(tracing::Span::current()),
    )
}

#[allow(unused)]
pub struct VoiceHandle {
    client_id: Uuid,
    channel_id: Uuid,
    status_handle: tokio::task::JoinHandle<()>,
    voice_host: Arc<dyn VoiceHost>,
    room_handle: ChatRoomJoined,
    source_id: u32,
}

impl Future for VoiceHandle {
    type Output = anyhow::Result<()>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.status_handle.poll_unpin(cx).map_err(Into::into)
    }
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        let voice_host_handle = self.voice_host.clone();
        let channel_id = self.channel_id;
        let client_id = self.client_id;
        tokio::spawn(async move { voice_host_handle.leave(channel_id, client_id).await });
        self.status_handle.abort();
    }
}
