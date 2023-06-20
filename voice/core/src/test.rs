use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::Context;
use assert_matches::assert_matches;
use tokio::net::UdpSocket;
use udp_proto::UdpWithBuf;
use uuid::Uuid;
use voice_proto::{server_message, IpDiscoveryRequest, Ping, ServerMessage, ServerVoice};
use voices_voice_crypto::VoiceCrypto;
use xsalsa20poly1305::KeyInit;

use crate::channel::connection::VoiceConnection;
use crate::config::VoiceServerConfig;
use crate::{
    ConnectionState, EstablishSession, OpenConnection, Peer, StatusError, VoiceServerImpl,
};

#[tokio::test]
async fn test_assign_chan() {
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let first_port = srv.ports.start();
    srv.assign_channel(chan_id).await.unwrap();
    let chan = srv.get_channel(chan_id).await.unwrap();
    assert!(chan.healthy());
    assert_eq!(first_port, chan.port());
    let status = srv.status(chan_id).await.unwrap();
    assert!(status.is_empty());
}

#[tokio::test]
async fn test_chan_ports_exhausted() {
    let srv = create_srv().await.unwrap();
    let first_port = srv.ports.start();
    for p in first_port..first_port + srv.ports.total() {
        let id = Uuid::new_v4();
        let chan = assert_matches!(srv.assign_channel(id).await, Ok(chan) => chan);
        assert!(chan.healthy());
        assert_eq!(p, chan.port());
    }
    let id = Uuid::new_v4();
    assert_matches!(srv.assign_channel(id).await, Err(_));
}

#[tokio::test]
async fn test_conn_open() {
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let port = srv.ports.start();
    let client_id = Uuid::new_v4();
    let resp = open(&srv, client_id, chan_id).await;
    assert_matches!(resp, Ok((_, remote_sock)) => {
            assert_eq!(remote_sock, SocketAddr::from((srv.host_addr, port)));
        }
    );
    assert_matches!(srv.status(chan_id).await, Ok(status) => {
        assert!(status.is_empty());
    });
    assert_matches!(
        srv.user_status(chan_id, client_id).await,
        Ok(ConnectionState::Waiting)
    );
}

#[tokio::test(start_paused = true)]
async fn test_conn_open_timeout() {
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id = Uuid::new_v4();
    let (_, _) = open(&srv, client_id, chan_id).await.unwrap();
    tokio::time::sleep(VoiceConnection::IDLE_TIMEOUT + Duration::from_secs(1)).await;
    assert_matches!(
        srv.user_status(chan_id, client_id).await,
        Err(StatusError::PeerNotFound(_))
    );
}

#[tokio::test]
async fn test_conn_open_and_establish() {
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id = Uuid::new_v4();
    let (source_id, _) = open(&srv, client_id, chan_id).await.unwrap();

    let req = EstablishSession {
        channel_id: chan_id,
        client_id,
        client_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
    };
    assert_matches!(srv.establish_session(req).await, Ok(_));
    assert_matches!(
        srv.user_status(chan_id, client_id).await,
        Ok(ConnectionState::Peered)
    );
    let status = srv.status(chan_id).await.unwrap();
    assert_eq!(
        status,
        vec![Peer {
            id: client_id,
            source_id,
            name: client_id.to_string()
        }]
    );
}

#[tokio::test]
async fn test_conn_open_and_ip_disco() {
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id = Uuid::new_v4();
    let (source_id, remote_sock) = open(&srv, client_id, chan_id).await.unwrap();
    let client_ip = Ipv4Addr::LOCALHOST;
    let client_sock = UdpSocket::bind(SocketAddr::from((client_ip.to_ipv6_mapped(), 0)))
        .await
        .unwrap();
    let client_addr = client_sock.local_addr().unwrap();
    let mut sock = UdpWithBuf::new(client_sock);
    sock.connect(remote_sock).await.unwrap();
    let msg = voice_proto::ClientMessage::ip_disco(IpDiscoveryRequest {
        uuid: client_id.as_bytes().to_vec(),
        source_id,
    });
    sock.send(&msg).await.unwrap();
    let resp: Result<voice_proto::ServerMessage, _> =
        tokio::time::timeout(Duration::from_millis(200), sock.recv())
            .await
            .unwrap();
    assert_matches!(resp, Ok(
        voice_proto::ServerMessage {
            message: Some(
                voice_proto::server_message::Message::IpDisco(voice_proto::IpDiscoveryResponse {
                    ip, port
                })
            )
        }) => {
        match ip.parse::<IpAddr>().unwrap() {
            IpAddr::V4(v4) => {
                assert_eq!(client_ip, v4);
            },
            IpAddr::V6(v6) => {
                assert_eq!(client_ip.to_ipv6_mapped(), v6);
            }
        }
        assert_eq!(client_addr.port() as u32, port);
    })
}

#[tokio::test(start_paused = true)]
async fn test_conn_open_and_ping() {
    let client_sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let client_addr = client_sock.local_addr().unwrap();
    let mut sock = UdpWithBuf::new(client_sock);
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id = Uuid::new_v4();
    let (source_id, remote_sock, _) = open_and_establish(&srv, client_id, chan_id, client_addr)
        .await
        .unwrap();
    sock.connect(remote_sock).await.unwrap();

    for i in 0..5 {
        let msg = voice_proto::ClientMessage::ping(Ping {
            seq: i,
            recv: 0,
            sent: 0,
            source_id,
        });
        sock.send(&msg).await.unwrap();
        let resp: Result<voice_proto::ServerMessage, _> =
            tokio::time::timeout(Duration::from_millis(200), sock.recv())
                .await
                .unwrap();
        assert_matches!(
            resp,
            Ok(voice_proto::ServerMessage {
                message: Some(voice_proto::server_message::Message::Pong(
                    voice_proto::Pong {
                        seq,
                        received: 0,
                        sent: 0,
                    }
                ))
            }) => {
                assert_eq!(i, seq);
            }
        );
        tokio::time::advance(Duration::from_secs(5)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn test_cant_hijack_session() {
    let client_sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let client_addr1 = client_sock.local_addr().unwrap();
    let mut sock1 = UdpWithBuf::new(client_sock);
    let mut sock2 = UdpWithBuf::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id = Uuid::new_v4();
    let (source_id, remote_sock, _) = open_and_establish(&srv, client_id, chan_id, client_addr1)
        .await
        .unwrap();
    sock1.connect(remote_sock).await.unwrap();
    sock2.connect(remote_sock).await.unwrap();

    for i in 0..5 {
        let msg = voice_proto::ClientMessage::ping(Ping {
            seq: i,
            recv: 0,
            sent: 0,
            source_id,
        });
        sock1.send(&msg).await.unwrap();
        let resp: Result<voice_proto::ServerMessage, _> =
            tokio::time::timeout(Duration::from_millis(200), sock1.recv())
                .await
                .unwrap();
        assert_matches!(
            resp,
            Ok(voice_proto::ServerMessage {
                message: Some(voice_proto::server_message::Message::Pong(
                    voice_proto::Pong {
                        seq,
                        received: 0,
                        sent: 0,
                    }
                ))
            }) => {
                assert_eq!(i, seq);
            }
        );
        sock2.send(&msg).await.unwrap();
        let resp: Result<Result<voice_proto::ServerMessage, _>, _> =
            tokio::time::timeout(Duration::from_millis(200), sock2.recv()).await;
        assert_matches!(resp, Err(_));
        tokio::time::advance(Duration::from_secs(5)).await;
    }
}

#[tokio::test]
async fn test_conn_open_send_works() {
    let client_sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let client_addr = client_sock.local_addr().unwrap();
    let mut sock1 = UdpWithBuf::new(client_sock);
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id1 = Uuid::new_v4();
    let (source_id1, remote_sock, key) = open_and_establish(&srv, client_id1, chan_id, client_addr)
        .await
        .unwrap();
    sock1
        .connect(remote_sock)
        .await
        .context(remote_sock.to_string())
        .unwrap();
    let cipher1 = xsalsa20poly1305::XSalsa20Poly1305::new_from_slice(&key).unwrap();

    let client_sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let client_addr = client_sock.local_addr().unwrap();
    let mut sock2 = UdpWithBuf::new(client_sock);
    let client_id2 = Uuid::new_v4();
    let (source_id2, remote_sock, key) = open_and_establish(&srv, client_id2, chan_id, client_addr)
        .await
        .unwrap();
    sock2.connect(remote_sock).await.unwrap();
    let cipher2 = xsalsa20poly1305::XSalsa20Poly1305::new_from_slice(&key).unwrap();

    for sequence in 0..5 {
        let stream_time = 0;
        let mut payload = vec![1; 100];
        cipher1
            .encrypt(&mut payload, sequence, stream_time)
            .unwrap();
        let msg = voice_proto::ClientMessage::voice(voice_proto::Voice {
            source_id: source_id1,
            sequence,
            stream_time,
            payload,
        });
        sock1.send(&msg).await.unwrap();
    }

    for expected_sequence in 0..5 {
        let msg = tokio::select! {
            _ = sock1.recv::<voice_proto::ServerMessage>() => {
                panic!()
            },
            res = tokio::time::timeout(Duration::from_millis(500), sock2.recv::<voice_proto::ServerMessage>()) => {
                res.unwrap()
            }
        };
        assert_matches!(
            msg,
            Ok(ServerMessage {
                message: Some(server_message::Message::Voice(ServerVoice {
                    source_id,
                    sequence,
                    stream_time,
                    mut payload
                }))
            }) => {
                assert_eq!(source_id1, source_id);
                assert_eq!(expected_sequence, sequence);
                cipher2.decrypt(&mut payload, sequence, stream_time).unwrap();
                assert_eq!(payload, vec![1; 100]);
            }
        )
    }

    let msg = voice_proto::ClientMessage::ping(Ping {
        seq: 0,
        recv: 0,
        sent: 5,
        source_id: source_id1,
    });
    sock1.send(&msg).await.unwrap();
    let resp: Result<voice_proto::ServerMessage, _> =
        tokio::time::timeout(Duration::from_millis(200), sock1.recv())
            .await
            .unwrap();
    assert_matches!(
        resp,
        Ok(voice_proto::ServerMessage {
            message: Some(voice_proto::server_message::Message::Pong(
                voice_proto::Pong {
                    seq: 0,
                    received: 5,
                    sent: 0,
                }
            ))
        })
    );

    let msg = voice_proto::ClientMessage::ping(Ping {
        seq: 0,
        recv: 5,
        sent: 0,
        source_id: source_id2,
    });
    sock2.send(&msg).await.unwrap();
    let resp: Result<voice_proto::ServerMessage, _> =
        tokio::time::timeout(Duration::from_millis(200), sock2.recv())
            .await
            .unwrap();
    assert_matches!(
        resp,
        Ok(voice_proto::ServerMessage {
            message: Some(voice_proto::server_message::Message::Pong(
                voice_proto::Pong {
                    seq: 0,
                    received: 0,
                    sent: 5,
                }
            ))
        })
    );
}

#[tokio::test]
async fn test_bad_crypt_kills_conn() {
    let client_sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let client_addr = client_sock.local_addr().unwrap();
    let mut sock1 = UdpWithBuf::new(client_sock);
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id1 = Uuid::new_v4();
    let (source_id1, remote_sock, _) = open_and_establish(&srv, client_id1, chan_id, client_addr)
        .await
        .unwrap();
    sock1.connect(remote_sock).await.unwrap();
    let stream_time = 0;
    let payload = vec![1; 100];
    let msg = voice_proto::ClientMessage::voice(voice_proto::Voice {
        source_id: source_id1,
        sequence: 0,
        stream_time,
        payload,
    });
    sock1.send(&msg).await.unwrap();

    let msg = voice_proto::ClientMessage::ping(Ping {
        seq: 0,
        recv: 0,
        sent: 1,
        source_id: source_id1,
    });
    sock1.send(&msg).await.unwrap();
    let resp: Result<Result<voice_proto::ServerMessage, _>, _> =
        tokio::time::timeout(Duration::from_millis(200), sock1.recv()).await;
    assert_matches!(resp, Err(_));
}

#[tokio::test(start_paused = true)]
async fn test_conn_open_and_establish_timeout() {
    let (srv, chan_id) = setup_with_channel().await.unwrap();
    let client_id = Uuid::new_v4();
    let client_addr = SocketAddr::from(([127, 0, 0, 1], 0));
    open_and_establish(&srv, client_id, chan_id, client_addr)
        .await
        .unwrap();
    assert_matches!(
        srv.user_status(chan_id, client_id).await,
        Ok(ConnectionState::Peered)
    );
    tokio::time::sleep(VoiceConnection::IDLE_TIMEOUT + Duration::from_secs(1)).await;
    assert_matches!(
        srv.user_status(chan_id, client_id).await,
        Err(StatusError::PeerNotFound(_))
    );
}

async fn setup_with_channel() -> anyhow::Result<(VoiceServerImpl, Uuid)> {
    let srv = create_srv().await?;
    let chan_id = Uuid::new_v4();
    srv.assign_channel(chan_id).await?;
    Ok((srv, chan_id))
}

async fn open(
    srv: &VoiceServerImpl,
    client_id: Uuid,
    chan_id: Uuid,
) -> anyhow::Result<(u32, SocketAddr)> {
    let req = OpenConnection {
        user_name: client_id.to_string(),
        channel_id: chan_id,
        client_id,
    };
    let conn = srv.open_connection(req).await?;
    Ok((conn.source_id, conn.sock))
}

async fn open_and_establish(
    srv: &VoiceServerImpl,
    client_id: Uuid,
    chan_id: Uuid,
    client_addr: SocketAddr,
) -> anyhow::Result<(u32, SocketAddr, Vec<u8>)> {
    let (source_id, sock) = open(srv, client_id, chan_id).await?;
    let req = EstablishSession {
        channel_id: chan_id,
        client_id,
        client_addr,
    };
    let established = srv.establish_session(req).await?;
    Ok((source_id, sock, established.crypt_key))
}

async fn create_srv() -> anyhow::Result<VoiceServerImpl> {
    init_tracing();
    let voice_cfg = VoiceServerConfig {
        first_udp_port: 0,
        udp_ports: 5,
        udp_host: "localhost".into(),
    };
    voice_cfg.server().await
}

fn init_tracing() {
    let v = std::env::var("TEST_LOG").ok().is_some();
    use tracing_subscriber::prelude::*;
    if v {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(tracing_subscriber::EnvFilter::new(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "DEBUG".into()),
                ))
                .with(tracing_subscriber::fmt::layer()),
        );
        let _ = tracing_log::LogTracer::init();
    }
}
