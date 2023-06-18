#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AssignChannelRequest {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AssignChannelResponse {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StatusRequest {
    #[prost(string, tag = "1")]
    pub channel_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StatusResponse {
    #[prost(message, repeated, tag = "1")]
    pub info: ::prost::alloc::vec::Vec<ClientInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientInfo {
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub src_id: u32,
    #[prost(string, tag = "3")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenConnectionRequest {
    #[prost(string, tag = "1")]
    pub user_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub user_id: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub channel_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenConnectionResponse {
    #[prost(message, optional, tag = "1")]
    pub udp_sock: ::core::option::Option<SockAddr>,
    #[prost(uint32, tag = "2")]
    pub src_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EstablishSessionRequest {
    #[prost(string, tag = "1")]
    pub user_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub client_sock: ::core::option::Option<SockAddr>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EstablishSessionResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub crypt_key: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SockAddr {
    #[prost(string, tag = "1")]
    pub ip: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub port: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaveRequest {
    #[prost(string, tag = "1")]
    pub user_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub channel_id: ::prost::alloc::string::String,
}
/// Ok / Error
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LeaveResponse {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UserStatusRequest {
    #[prost(string, tag = "1")]
    pub user_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub channel_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UserStatusResponse {
    #[prost(oneof = "user_status_response::Status", tags = "1, 2, 3")]
    pub status: ::core::option::Option<user_status_response::Status>,
}
/// Nested message and enum types in `UserStatusResponse`.
pub mod user_status_response {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Status {
        #[prost(message, tag = "1")]
        Waiting(()),
        #[prost(message, tag = "2")]
        Peered(()),
        #[prost(message, tag = "3")]
        Error(()),
    }
}
/// Generated server implementations.
pub mod voice_server_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with VoiceServerServer.
    #[async_trait]
    pub trait VoiceServer: Send + Sync + 'static {
        async fn open_connection(
            &self,
            request: tonic::Request<super::OpenConnectionRequest>,
        ) -> Result<tonic::Response<super::OpenConnectionResponse>, tonic::Status>;
        async fn establish_session(
            &self,
            request: tonic::Request<super::EstablishSessionRequest>,
        ) -> Result<tonic::Response<super::EstablishSessionResponse>, tonic::Status>;
        async fn leave(
            &self,
            request: tonic::Request<super::LeaveRequest>,
        ) -> Result<tonic::Response<super::LeaveResponse>, tonic::Status>;
        async fn user_status(
            &self,
            request: tonic::Request<super::UserStatusRequest>,
        ) -> Result<tonic::Response<super::UserStatusResponse>, tonic::Status>;
        async fn status(
            &self,
            request: tonic::Request<super::StatusRequest>,
        ) -> Result<tonic::Response<super::StatusResponse>, tonic::Status>;
        async fn assign_channel(
            &self,
            request: tonic::Request<super::AssignChannelRequest>,
        ) -> Result<tonic::Response<super::AssignChannelResponse>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct VoiceServerServer<T: VoiceServer> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: VoiceServer> VoiceServerServer<T> {
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
        pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
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
    impl<T, B> tonic::codegen::Service<http::Request<B>> for VoiceServerServer<T>
    where
        T: VoiceServer,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/voice_server.v1.VoiceServer/OpenConnection" => {
                    #[allow(non_camel_case_types)]
                    struct OpenConnectionSvc<T: VoiceServer>(pub Arc<T>);
                    impl<T: VoiceServer> tonic::server::UnaryService<super::OpenConnectionRequest>
                        for OpenConnectionSvc<T>
                    {
                        type Response = super::OpenConnectionResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::OpenConnectionRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).open_connection(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = OpenConnectionSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_server.v1.VoiceServer/EstablishSession" => {
                    #[allow(non_camel_case_types)]
                    struct EstablishSessionSvc<T: VoiceServer>(pub Arc<T>);
                    impl<T: VoiceServer> tonic::server::UnaryService<super::EstablishSessionRequest>
                        for EstablishSessionSvc<T>
                    {
                        type Response = super::EstablishSessionResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::EstablishSessionRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).establish_session(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = EstablishSessionSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_server.v1.VoiceServer/Leave" => {
                    #[allow(non_camel_case_types)]
                    struct LeaveSvc<T: VoiceServer>(pub Arc<T>);
                    impl<T: VoiceServer> tonic::server::UnaryService<super::LeaveRequest> for LeaveSvc<T> {
                        type Response = super::LeaveResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::LeaveRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).leave(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = LeaveSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_server.v1.VoiceServer/UserStatus" => {
                    #[allow(non_camel_case_types)]
                    struct UserStatusSvc<T: VoiceServer>(pub Arc<T>);
                    impl<T: VoiceServer> tonic::server::UnaryService<super::UserStatusRequest> for UserStatusSvc<T> {
                        type Response = super::UserStatusResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::UserStatusRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).user_status(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = UserStatusSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_server.v1.VoiceServer/Status" => {
                    #[allow(non_camel_case_types)]
                    struct StatusSvc<T: VoiceServer>(pub Arc<T>);
                    impl<T: VoiceServer> tonic::server::UnaryService<super::StatusRequest> for StatusSvc<T> {
                        type Response = super::StatusResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::StatusRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).status(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = StatusSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/voice_server.v1.VoiceServer/AssignChannel" => {
                    #[allow(non_camel_case_types)]
                    struct AssignChannelSvc<T: VoiceServer>(pub Arc<T>);
                    impl<T: VoiceServer> tonic::server::UnaryService<super::AssignChannelRequest>
                        for AssignChannelSvc<T>
                    {
                        type Response = super::AssignChannelResponse;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::AssignChannelRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).assign_channel(request).await };
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
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(empty_body())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: VoiceServer> Clone for VoiceServerServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: VoiceServer> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: VoiceServer> tonic::server::NamedService for VoiceServerServer<T> {
        const NAME: &'static str = "voice_server.v1.VoiceServer";
    }
}
