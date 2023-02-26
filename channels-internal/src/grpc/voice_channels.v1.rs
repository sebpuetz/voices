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
/// Generated server implementations.
pub mod channels_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with ChannelsServer.
    #[async_trait]
    pub trait Channels: Send + Sync + 'static {
        async fn create_server(
            &self,
            request: tonic::Request<super::CreateServerRequest>,
        ) -> Result<tonic::Response<super::CreateServerResponse>, tonic::Status>;
        async fn get_server(
            &self,
            request: tonic::Request<super::GetServerRequest>,
        ) -> Result<tonic::Response<super::GetServerResponse>, tonic::Status>;
        async fn get_servers(
            &self,
            request: tonic::Request<super::GetServersRequest>,
        ) -> Result<tonic::Response<super::GetServersResponse>, tonic::Status>;
        async fn create_channel(
            &self,
            request: tonic::Request<super::CreateChannelRequest>,
        ) -> Result<tonic::Response<super::CreateChannelResponse>, tonic::Status>;
        async fn get_channel(
            &self,
            request: tonic::Request<super::GetChannelRequest>,
        ) -> Result<tonic::Response<super::GetChannelResponse>, tonic::Status>;
        async fn assign_channel(
            &self,
            request: tonic::Request<super::AssignChannelRequest>,
        ) -> Result<tonic::Response<super::AssignChannelResponse>, tonic::Status>;
        async fn unassign_channel(
            &self,
            request: tonic::Request<super::UnassignChannelRequest>,
        ) -> Result<tonic::Response<super::UnassignChannelResponse>, tonic::Status>;
        async fn register_voice_server(
            &self,
            request: tonic::Request<super::RegisterVoiceServerRequest>,
        ) -> Result<tonic::Response<super::RegisterVoiceServerResponse>, tonic::Status>;
        async fn cleanup_stale_voice_servers(
            &self,
            request: tonic::Request<super::CleanupStaleVoiceServersRequest>,
        ) -> Result<
            tonic::Response<super::CleanupStaleVoiceServersResponse>,
            tonic::Status,
        >;
    }
    #[derive(Debug)]
    pub struct ChannelsServer<T: Channels> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Channels> ChannelsServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for ChannelsServer<T>
    where
        T: Channels,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/voice_channels.v1.Channels/CreateServer" => {
                    #[allow(non_camel_case_types)]
                    struct CreateServerSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::CreateServerRequest>
                    for CreateServerSvc<T> {
                        type Response = super::CreateServerResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::CreateServerRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).create_server(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = CreateServerSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/GetServer" => {
                    #[allow(non_camel_case_types)]
                    struct GetServerSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::GetServerRequest>
                    for GetServerSvc<T> {
                        type Response = super::GetServerResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetServerRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).get_server(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetServerSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/GetServers" => {
                    #[allow(non_camel_case_types)]
                    struct GetServersSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::GetServersRequest>
                    for GetServersSvc<T> {
                        type Response = super::GetServersResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetServersRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).get_servers(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetServersSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/CreateChannel" => {
                    #[allow(non_camel_case_types)]
                    struct CreateChannelSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::CreateChannelRequest>
                    for CreateChannelSvc<T> {
                        type Response = super::CreateChannelResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::CreateChannelRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).create_channel(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = CreateChannelSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/GetChannel" => {
                    #[allow(non_camel_case_types)]
                    struct GetChannelSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::GetChannelRequest>
                    for GetChannelSvc<T> {
                        type Response = super::GetChannelResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetChannelRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).get_channel(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetChannelSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/AssignChannel" => {
                    #[allow(non_camel_case_types)]
                    struct AssignChannelSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::AssignChannelRequest>
                    for AssignChannelSvc<T> {
                        type Response = super::AssignChannelResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::AssignChannelRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).assign_channel(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = AssignChannelSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/UnassignChannel" => {
                    #[allow(non_camel_case_types)]
                    struct UnassignChannelSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::UnassignChannelRequest>
                    for UnassignChannelSvc<T> {
                        type Response = super::UnassignChannelResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::UnassignChannelRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).unassign_channel(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = UnassignChannelSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/RegisterVoiceServer" => {
                    #[allow(non_camel_case_types)]
                    struct RegisterVoiceServerSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::RegisterVoiceServerRequest>
                    for RegisterVoiceServerSvc<T> {
                        type Response = super::RegisterVoiceServerResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RegisterVoiceServerRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).register_voice_server(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = RegisterVoiceServerSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_channels.v1.Channels/CleanupStaleVoiceServers" => {
                    #[allow(non_camel_case_types)]
                    struct CleanupStaleVoiceServersSvc<T: Channels>(pub Arc<T>);
                    impl<
                        T: Channels,
                    > tonic::server::UnaryService<super::CleanupStaleVoiceServersRequest>
                    for CleanupStaleVoiceServersSvc<T> {
                        type Response = super::CleanupStaleVoiceServersResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::CleanupStaleVoiceServersRequest,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).cleanup_stale_voice_servers(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = CleanupStaleVoiceServersSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Channels> Clone for ChannelsServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Channels> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Channels> tonic::server::NamedService for ChannelsServer<T> {
        const NAME: &'static str = "voice_channels.v1.Channels";
    }
}
