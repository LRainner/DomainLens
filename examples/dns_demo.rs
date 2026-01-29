use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_proto::rr::{rdata::A, Name, RData, Record, RecordType};
use tokio::{net::UdpSocket, sync::Semaphore};

type DynError = Box<dyn Error + Send + Sync>;

/// 一个“工程化但仍然最小”的 UDP DNS 服务器参考实现：
/// - tokio + async UDP I/O；
/// - hickory-proto 做报文解析/编码；
/// - 分层：main(监听+并发控制) -> handle_packet(解析/回包) -> build_response(业务逻辑)；
/// - 用 Semaphore 控制 spawn 并发，避免 UDP 洪泛导致任务无限增长/OOM；
///
/// 运行：
///   cargo run --example dns_best_practice
///
/// 测试：
///   dig @127.0.0.1 -p 5300 example.com A
#[tokio::main]
async fn main() -> Result<(), DynError> {
    let addr: SocketAddr = "0.0.0.0:5300".parse()?;
    let socket = Arc::new(UdpSocket::bind(addr).await?);

    // 并发上限（示例值）：防止大量 UDP 包导致 spawn 过多任务占满内存
    let sem = Arc::new(Semaphore::new(512));

    eprintln!("[dns] start");
    eprintln!("[dns] listening on {addr} (udp)");

    // 512 是传统 DNS UDP 包大小；这里放宽到 2048
    let mut buf = vec![0u8; 2048];

    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        eprintln!("[dns] recv from {peer}, bytes={len}");

        let payload = buf[..len].to_vec();

        let socket = socket.clone();
        let sem = sem.clone();

        // 获取并发许可：如果超限，将在这里 backpressure
        let permit = sem.acquire_owned().await?;

        tokio::spawn(async move {
            let _permit = permit; // 让 permit 覆盖整个任务生命周期

            if let Err(e) = handle_packet(socket, peer, payload).await {
                eprintln!("[dns] handle_packet error from {peer}: {e}");
            } else {
                eprintln!("[dns] done peer={peer}");
            }
        });
    }
}

/// 单个 UDP 包的处理：
/// - 解析请求
/// - 构造响应
/// - 发送响应
async fn handle_packet(
    socket: Arc<UdpSocket>,
    peer: SocketAddr,
    payload: Vec<u8>,
) -> Result<(), DynError> {
    let req = match Message::from_vec(&payload) {
        Ok(m) => m,
        Err(_) => {
            eprintln!("[dns] parse failed peer={peer}");
            // 解析失败：多数情况下直接丢弃就行（FORMERR 需要 id，不一定能拿到）
            return Ok(());
        }
    };

    if let Some(q) = req.queries().first() {
        eprintln!(
            "[dns] query peer={peer} qname={} qtype={:?}",
            q.name().to_ascii(),
            q.query_type()
        );
    } else {
        eprintln!("[dns] query peer={peer} <no-question>");
    }

    let resp = build_response(&req)?;
    let bytes = resp.to_vec()?;
    socket.send_to(&bytes, peer).await?;
    eprintln!("[dns] sent to {peer}, bytes={}", bytes.len());

    Ok(())
}

/// 业务逻辑层：把请求映射为响应。
///
/// 当前 demo 行为：
/// - 如果第一个 Question 是 A 记录查询，返回 127.0.0.1；
/// - 否则不返回 answer，但仍返回 NoError；
/// - 如果没有 Question，返回 FormErr。
fn build_response(req: &Message) -> Result<Message, DynError> {
    let mut resp = Message::new();

    resp.set_id(req.id());
    resp.set_message_type(MessageType::Response);
    resp.set_op_code(OpCode::Query);
    resp.set_recursion_desired(req.recursion_desired());
    resp.set_recursion_available(true);
    resp.set_authoritative(false);

    // 一般需要把原始 queries 带回去，客户端才能对得上响应
    for q in req.queries() {
        resp.add_query(q.clone());
    }

    if let Some(q) = req.queries().first() {
        if q.query_type() == RecordType::A {
            let qname_ascii = q.name().to_ascii();
            let name = Name::from_ascii(&qname_ascii)?;
            let ip = Ipv4Addr::new(127, 0, 0, 1);

            resp.add_answer(Record::from_rdata(name, 60, RData::A(A::from(ip))));
        }

        resp.set_response_code(ResponseCode::NoError);
    } else {
        resp.set_response_code(ResponseCode::FormErr);
    }

    Ok(resp)
}