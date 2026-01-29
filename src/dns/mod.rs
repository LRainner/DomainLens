use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_proto::rr::{rdata::A, Name, RData, Record, RecordType};
use tokio::net::UdpSocket;

use crate::domain::DomainRule;

/// 极简 DNS 服务器：
/// - 基于 tokio 的 UDP 监听；
/// - 使用 hickory-proto 解析/构造 DNS 报文；
/// - 当前实现：对任意 A 记录查询统一返回 127.0.0.1，后续可在 handle_query 中接入 DomainRule。
pub struct DnsServer {
    addr: SocketAddr,
    rule: Arc<DomainRule>,
}

impl DnsServer {
    pub fn new(addr: SocketAddr, rule: Arc<DomainRule>) -> Self {
        Self { addr, rule }
    }

    /// 启动 UDP DNS 监听循环（单线程循环处理，简单稳定）。
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let socket = UdpSocket::bind(self.addr).await?;
        println!("DNS server listening on {}", self.addr);

        let mut buf = vec![0u8; 512];

        loop {
            let (len, peer) = socket.recv_from(&mut buf).await?;
            let payload = buf[..len].to_vec();
            let rule = self.rule.clone();

            // 这里不再 clone socket，而是在同一任务内顺序处理：
            // 先解析 + 组包，再用收到的同一个 socket 回发响应。
            if let Err(e) = handle_query(&socket, peer, payload, rule).await {
                eprintln!("failed to handle query from {}: {:?}", peer, e);
            }
        }
    }
}

async fn handle_query(
    socket: &UdpSocket,
    peer: SocketAddr,
    payload: Vec<u8>,
    rule: Arc<DomainRule>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. 解析 DNS 报文
    let msg = Message::from_vec(&payload)?;
    let mut resp = Message::new();
    resp.set_id(msg.id());
    resp.set_message_type(MessageType::Response);
    resp.set_op_code(OpCode::Query);
    resp.set_recursion_desired(msg.recursion_desired());
    resp.set_recursion_available(true);
    resp.set_authoritative(true);

    // 把原始 queries 拷贝到响应里。
    for q in msg.queries() {
        resp.add_query(q.clone());
    }

    // 当前只处理第一个 Question。
    if let Some(query) = msg.queries().first() {
        let qname = query.name().to_ascii();
        let qtype = query.query_type();

        println!("received query from {}: {} {:?}", peer, qname, qtype);

        // TODO: 在这里接入 DomainRule：
        // if rule.search_domain(&qname).is_some() { ... }
        let _ = &rule; // 先避免未使用警告

        // 简单 demo：如果是 A 记录查询，返回 127.0.0.1。
        if qtype == RecordType::A {
            let name = Name::from_ascii(&qname)?;
            let ip: Ipv4Addr = "127.0.0.1".parse()?;
            let rdata = RData::A(A::from(ip));

            let record = Record::from_rdata(name, 60, rdata);
            resp.add_answer(record);
        }

        resp.set_response_code(ResponseCode::NoError);
    } else {
        resp.set_response_code(ResponseCode::FormErr);
    }

    // 2. 序列化响应 —— 直接使用 Message::to_vec()
    let resp_buf = resp.to_vec()?;

    // 3. 发送响应
    socket.send_to(&resp_buf, &peer).await?;

    Ok(())
}


pub mod server;