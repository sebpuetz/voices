#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CleanupStaleVoiceServersRequest {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CleanupStaleVoiceServersResponse {
    #[prost(string, repeated, tag = "1")]
    pub deleted: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegisterVoiceServerRequest {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub addr: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RegisterVoiceServerResponse {
    #[prost(message, optional, tag = "1")]
    pub valid_until: ::core::option::Option<::prost_types::Timestamp>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetServersRequest {
    #[prost(int64, optional, tag = "1")]
    pub per_page: ::core::option::Option<i64>,
    #[prost(int64, optional, tag = "2")]
    pub page: ::core::option::Option<i64>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetServersResponse {
    #[prost(message, repeated, tag = "1")]
    pub info: ::prost::alloc::vec::Vec<get_servers_response::ServerInfo>,
    #[prost(int64, tag = "2")]
    pub pages: i64,
}
/// Nested message and enum types in `GetServersResponse`.
pub mod get_servers_response {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ServerInfo {
        #[prost(string, tag = "1")]
        pub server_id: ::prost::alloc::string::String,
        #[prost(string, tag = "2")]
        pub name: ::prost::alloc::string::String,
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetServerRequest {
    #[prost(string, tag = "1")]
    pub server_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetServerResponse {
    #[prost(string, tag = "1")]
    pub server_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub channels: ::prost::alloc::vec::Vec<ChannelInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChannelInfo {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateServerRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateServerResponse {
    #[prost(string, tag = "1")]
    pub server_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateChannelRequest {
    #[prost(string, tag = "1")]
    pub server_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateChannelResponse {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetChannelRequest {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetChannelResponse {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "3")]
    pub voice_server_host: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AssignChannelRequest {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub reassign: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AssignChannelResponse {
    #[prost(string, tag = "1")]
    pub voice_server_host: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnassignChannelRequest {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub server_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnassignChannelResponse {}
/// Generated client implementations.
pub mod channels_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    #[derive(Debug, Clone)]
    pub struct ChannelsClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl ChannelsClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> ChannelsClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> ChannelsClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            ChannelsClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        pub async fn create_server(
            &mut self,
            request: impl tonic::IntoRequest<super::CreateServerRequest>,
        ) -> Result<tonic::Response<super::CreateServerResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/CreateServer",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn get_server(
            &mut self,
            request: impl tonic::IntoRequest<super::GetServerRequest>,
        ) -> Result<tonic::Response<super::GetServerResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/GetServer",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn get_servers(
            &mut self,
            request: impl tonic::IntoRequest<super::GetServersRequest>,
        ) -> Result<tonic::Response<super::GetServersResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/GetServers",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn create_channel(
            &mut self,
            request: impl tonic::IntoRequest<super::CreateChannelRequest>,
        ) -> Result<tonic::Response<super::CreateChannelResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/CreateChannel",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn get_channel(
            &mut self,
            request: impl tonic::IntoRequest<super::GetChannelRequest>,
        ) -> Result<tonic::Response<super::GetChannelResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/GetChannel",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn assign_channel(
            &mut self,
            request: impl tonic::IntoRequest<super::AssignChannelRequest>,
        ) -> Result<tonic::Response<super::AssignChannelResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/AssignChannel",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn unassign_channel(
            &mut self,
            request: impl tonic::IntoRequest<super::UnassignChannelRequest>,
        ) -> Result<tonic::Response<super::UnassignChannelResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/UnassignChannel",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn register_voice_server(
            &mut self,
            request: impl tonic::IntoRequest<super::RegisterVoiceServerRequest>,
        ) -> Result<tonic::Response<super::RegisterVoiceServerResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/RegisterVoiceServer",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn cleanup_stale_voice_servers(
            &mut self,
            request: impl tonic::IntoRequest<super::CleanupStaleVoiceServersRequest>,
        ) -> Result<
            tonic::Response<super::CleanupStaleVoiceServersResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/voice_channels.v1.Channels/CleanupStaleVoiceServers",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}