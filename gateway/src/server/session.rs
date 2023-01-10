use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use anyhow::Context;
use futures_util::{Future, FutureExt};
use tokio::select;
use tracing::Instrument;
use uuid::Uuid;
use voice_server::grpc::{proto, tonic};
use voice_server::VoiceServer;
use voices_ws_proto::{ClientEvent, Init, Join, Present};

use crate::server::channels::{Channel, ChannelEvent, ChannelEventKind, Channels, ClientInfo};
use crate::server::ws::ControlStream;

use super::channel_registry::ChannelRegistry;
use super::channel_state::ChannelState;
use super::channels::ChatRoomJoined;
use super::ws::ControlStreamError;

/// Representation of a client session
///
/// Owns a handle to a [`VoiceServer`] and the known [`Channels`].
///
/// Has a [`VoiceHandle`] if the client is currently in a channel.
pub struct ServerSession<R, S>
where
    R: ChannelRegistry,
    S: ChannelState,
{
    // client info, received in init msg
    client_id: Uuid,
    client_name: String,
    // eventloop / backwards chan
    ctl: ControlStream,
    // optionally connected channel
    voice: Option<VoiceHandle<R::Voice, S>>,

    // channel registry
    channels: Channels<S, R>,
}

impl<R, S> ServerSession<R, S>
where
    R: ChannelRegistry,
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
            ChannelEventKind::Joined(joined) => {
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
                let room = self.channels.get_or_init(channel_id).await?;
                let room_id = room.id();
                if let Some(voice) = self.initialize_voice_connection(room).await? {
                    self.voice = Some(voice);
                } else {
                    self.ctl.join_error(room_id).await?;
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
        mut channel: Channel<S, R::Voice>,
    ) -> anyhow::Result<Option<VoiceHandle<R::Voice, S>>> {
        tracing::debug!("announcing udp");
        // should return source_id
        let channel_id = channel.id();
        let client_id = self.client_id;
        let msg = proto::OpenConnectionRequest {
            user_name: self.client_name.clone(),
            user_id: self.client_id.to_string(),
            channel_id: channel_id.to_string(),
        };
        let req = tonic::Request::new(msg.clone());
        let resp = match channel.voice().open_connection(req).await {
            Ok(o) => o.into_inner(),
            Err(e) => {
                if e.code() == tonic::Code::NotFound {
                    channel = self.channels.reassign_voice(channel_id).await?;
                    let req = tonic::Request::new(msg);
                    channel.voice().open_connection(req).await?.into_inner()
                } else {
                    tracing::warn!("open conn failed {:?}", e);
                    return Err(e.into());
                }
            }
        };
        let addr = resp.udp_sock.context("Missing response field")?;
        let port = addr.port as u16;
        let ip = IpAddr::from_str(&addr.ip).context("Invalid IpAddr")?;
        let udp_addr = SocketAddr::from((ip, port));
        tracing::debug!("udp running on {:?}", udp_addr);
        self.ctl.announce_udp(udp_addr).await?;
        tracing::debug!("announced udp, waiting for client udp addr");

        let client_udp = self.ctl.await_client_udp().await?;
        let client_udp = SocketAddr::from((client_udp.ip, client_udp.port));
        anyhow::ensure!(!client_udp.ip().is_unspecified());
        tracing::debug!("received client udp addr {client_udp}");

        let req = proto::EstablishSessionRequest {
            user_id: self.client_id.to_string(),
            channel_id: channel_id.to_string(),
            client_sock: Some(proto::SockAddr {
                port: client_udp.port() as _,
                ip: client_udp.ip().to_string(),
            }),
        };
        let req = tonic::Request::new(req);
        let resp = match channel.voice().establish_session(req).await {
            Ok(o) => o.into_inner(),
            Err(e) => {
                if e.code() == tonic::Code::NotFound {}
                tracing::warn!("open conn failed {:?}", e);
                return Err(e.into());
            }
        };
        let source_id = resp.src_id;
        let crypt_key = base64::encode(resp.crypt_key).into();
        tracing::debug!("successfully notified udp task");
        let req = proto::StatusRequest {
            channel_id: channel_id.to_string(),
        };
        let status_request = tonic::Request::new(req);
        let present = match channel.voice().status(status_request).await {
            Ok(o) => o
                .into_inner()
                .info
                .into_iter()
                .filter_map(|v| {
                    (v.client_id.parse::<Uuid>().ok()? != client_id).then_some(Present {
                        user: v.name,
                        source_id: v.src_id,
                    })
                })
                .collect(),
            Err(e) => {
                tracing::warn!("list present failed {:?}", e);
                return Err(e.into());
            }
        };

        let ready = voices_ws_proto::Ready {
            id: channel_id,
            src_id: source_id,
            present,
            crypt_key,
        };
        self.ctl.voice_ready(ready).await?;
        let voice_handle = channel.voice().clone();
        let status_handle = status_check(voice_handle.clone(), client_id, channel_id);
        let info = ClientInfo {
            client_id,
            source_id,
            name: self.client_name.clone(),
        };
        let room_handle = channel.join(info).await?;
        Ok(Some(VoiceHandle {
            client_id,
            channel_id,
            status_handle,
            voice_server: voice_handle,
            room_handle,
            source_id,
        }))
    }
}

fn status_check<V: VoiceServer>(
    voice_handle: V,
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
                let req = tonic::Request::new(proto::UserStatusRequest {
                    user_id: client_id.to_string(),
                    channel_id: channel_id.to_string(),
                });
                match voice_handle
                    .user_status(req)
                    .await
                    .map(|req| req.into_inner().status)
                {
                    Ok(Some(proto::user_status_response::Status::Peered(()))) => {
                        tracing::trace!("voice connection is in peered state");
                        continue;
                    }
                    Ok(Some(proto::user_status_response::Status::Waiting(()))) => {
                        tracing::trace!("voice connection is in waiting state");
                        continue;
                    }
                    Ok(Some(proto::user_status_response::Status::Error(()))) | Ok(None) => {
                        tracing::info!("voice connection errored");
                        break;
                    }
                    Err(status) => {
                        if let tonic::Code::NotFound = status.code() {
                            tracing::info!("didn't find voice connection");
                            break;
                        } else {
                            tracing::warn!("status request failed");
                            break;
                        }
                    }
                }
            }
        }
        .instrument(tracing::Span::current()),
    )
}

#[allow(unused)]
pub struct VoiceHandle<V, S>
where
    V: VoiceServer + Clone,
    S: ChannelState,
{
    client_id: Uuid,
    channel_id: Uuid,
    status_handle: tokio::task::JoinHandle<()>,
    voice_server: V,
    room_handle: ChatRoomJoined<S>,
    source_id: u32,
}

impl<V, S> Future for VoiceHandle<V, S>
where
    V: VoiceServer + Clone + Unpin,
    S: ChannelState + Unpin,
{
    type Output = anyhow::Result<()>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.status_handle.poll_unpin(cx).map_err(Into::into)
    }
}

impl<V, S> Drop for VoiceHandle<V, S>
where
    V: VoiceServer + Clone,
    S: ChannelState,
{
    fn drop(&mut self) {
        let req = tonic::Request::new(proto::LeaveRequest {
            user_id: self.client_id.to_string(),
            channel_id: self.channel_id.to_string(),
        });
        let handle = self.voice_server.clone();
        tokio::spawn(async move { handle.leave(req).await });
        self.status_handle.abort();
    }
}
