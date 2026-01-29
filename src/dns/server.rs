use std::{error::Error, net::{Ipv4Addr, SocketAddr}, sync::{Arc, atomic::{AtomicBool, Ordering}}};

use hickory_proto::{op::{Message, MessageType, OpCode, ResponseCode}, rr::{Name, RData, Record, RecordType, rdata::A}};
use tokio::{net::UdpSocket, sync::Semaphore};

use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
pub trait DnsServer: Send + Sync {
    async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn shutdown(&self);

    fn local_addr(&self) -> SocketAddr;
    fn is_running(&self) -> bool;
}

#[derive(Clone)]
pub struct DnsConfig {
    pub listen: SocketAddr,
    pub max_concurrency: usize,
    pub udp_buf_size: usize,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:5300".parse().unwrap(),
            max_concurrency: 512,
            udp_buf_size: 2048,
        }
    }
}

#[async_trait::async_trait]
pub trait DnsRequestHandler: Send + Sync {
    /// 返回 Some(resp)：发送响应
    /// 返回 None：静默丢弃
    async fn handle(
        &self,
        req: Message,
        peer: SocketAddr,
    ) -> Option<Message>;
}

type DynError = Box<dyn Error + Send + Sync>;

pub struct DnsServerImpl {
    socket: Arc<UdpSocket>,
    semaphore: Arc<Semaphore>,

    config: Arc<DnsConfig>,
    handler: Arc<dyn DnsRequestHandler>,

    shutdown: CancellationToken,
    running: AtomicBool,
}

#[async_trait::async_trait]
impl DnsServer for DnsServerImpl {
    async fn start(&self) -> Result<(), DynError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let socket = self.socket.clone();
        let sem = self.semaphore.clone();
        let handler = self.handler.clone();
        let shutdown = self.shutdown.clone();
        let buf_size = self.config.udp_buf_size;

        tokio::spawn(async move {
            let mut buf = vec![0u8; buf_size];

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        break;
                    }

                    res = socket.recv_from(&mut buf) => {
                        let (len, peer) = match res {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        let payload = buf[..len].to_vec();
                        let socket = socket.clone();
                        let handler = handler.clone();
                        let permit = match sem.clone().acquire_owned().await {
                            Ok(p) => p,
                            Err(_) => continue,
                        };

                        tokio::spawn(async move {
                            let _permit = permit;

                            let req = match Message::from_vec(&payload) {
                                Ok(m) => m,
                                Err(_) => return,
                            };

                            if let Some(resp) = handler.handle(req, peer).await {
                                if let Ok(bytes) = resp.to_vec() {
                                    let _ = socket.send_to(&bytes, peer).await;
                                }
                            }
                        });
                    }
                }
            }
        });

        Ok(())
    }

    async fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.shutdown.cancel();
    }

    fn local_addr(&self) -> SocketAddr {
        self.config.listen
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

pub struct DnsServerBuilder {
    config: DnsConfig,
    handler: Option<Arc<dyn DnsRequestHandler>>,
}

impl DnsServerBuilder {
    pub fn new(config: DnsConfig) -> Self {
        Self {
            config,
            handler: None,
        }
    }

    pub fn handler(mut self, handler: Arc<dyn DnsRequestHandler>) -> Self {
        self.handler = Some(handler);
        self
    }

    pub async fn build(self) -> Result<Arc<DnsServerImpl>, DynError> {
        let socket = UdpSocket::bind(self.config.listen).await?;

        Ok(Arc::new(DnsServerImpl {
            socket: Arc::new(socket),
            semaphore: Arc::new(Semaphore::new(self.config.max_concurrency)),
            config: Arc::new(self.config),
            handler: self.handler.expect("handler is required"),
            shutdown: CancellationToken::new(),
            running: AtomicBool::new(false),
        }))
    }
}


pub struct StaticAHandler;

#[async_trait::async_trait]
impl DnsRequestHandler for StaticAHandler {
    async fn handle(
        &self,
        req: Message,
        _peer: SocketAddr,
    ) -> Option<Message> {
        let q = req.queries().first()?;

        let mut resp = Message::new();
        resp.set_id(req.id());
        resp.set_message_type(MessageType::Response);
        resp.set_op_code(OpCode::Query);
        resp.set_recursion_available(true);
        resp.add_query(q.clone());

        if q.query_type() == RecordType::A {
            let name = Name::from_ascii(&q.name().to_ascii()).ok()?;
            resp.add_answer(
                Record::from_rdata(
                    name,
                    60,
                    RData::A(A::from(Ipv4Addr::new(127, 0, 0, 1))),
                )
            );
        }

        resp.set_response_code(ResponseCode::NoError);
        Some(resp)
    }
}