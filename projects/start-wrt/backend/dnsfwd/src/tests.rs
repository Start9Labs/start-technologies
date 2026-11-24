use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hickory_net::proto::op::{Edns, Message, MessageType, OpCode, Query, ResponseCode};
use hickory_net::proto::rr::rdata::opt::{EdnsCode, EdnsOption};
use hickory_net::proto::rr::rdata::{A, TXT};
use hickory_net::proto::rr::{Name, RData, Record, RecordType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use crate::config::{Config, Method, Server, Transport};
use crate::Forwarder;

#[derive(Clone, Default)]
struct Behavior {
    marker: u8,
    rcode: Option<ResponseCode>,
    delay: Duration,
    silent: bool,
    /// Answers with enough records to exceed 512 bytes.
    big: bool,
    /// Answers over UDP with the TC bit and no records.
    truncate_udp: bool,
    /// Closes each TCP connection after its first answer.
    close_tcp: bool,
}

/// Each query that a mock received, and whether it came over TCP.
type Log = Arc<Mutex<Vec<(Message, bool)>>>;

struct Mock {
    addr: SocketAddr,
    queries: Log,
}

impl Mock {
    async fn spawn(behavior: Behavior) -> Self {
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = udp.local_addr().unwrap();
        let tcp = TcpListener::bind(addr).await.unwrap();
        let queries = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new((behavior, queries.clone()));

        let udp_state = state.clone();
        tokio::spawn(async move {
            let udp = Arc::new(udp);
            let mut buf = vec![0u8; 4096];
            while let Ok((len, peer)) = udp.recv_from(&mut buf).await {
                let query = buf[..len].to_vec();
                let (udp, state) = (udp.clone(), udp_state.clone());
                tokio::spawn(async move {
                    if let Some(reply) = respond(&state, &query, false).await {
                        udp.send_to(&reply, peer).await.unwrap();
                    }
                });
            }
        });
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = tcp.accept().await {
                let state = state.clone();
                tokio::spawn(async move {
                    loop {
                        let mut len = [0u8; 2];
                        if stream.read_exact(&mut len).await.is_err() {
                            return;
                        }
                        let mut query = vec![0u8; usize::from(u16::from_be_bytes(len))];
                        stream.read_exact(&mut query).await.unwrap();
                        if let Some(reply) = respond(&state, &query, true).await {
                            let len = u16::try_from(reply.len()).unwrap().to_be_bytes();
                            stream
                                .write_all(&[&len, reply.as_slice()].concat())
                                .await
                                .unwrap();
                        }
                        if state.0.close_tcp {
                            return;
                        }
                    }
                });
            }
        });
        Self { addr, queries }
    }

    fn queries(&self) -> Vec<(Message, bool)> {
        self.queries.lock().unwrap().clone()
    }

    fn server(&self, timeout_ms: u64) -> Server {
        Server {
            transport: Transport::Plain,
            address: self.addr.ip(),
            port: Some(self.addr.port()),
            name: None,
            path: None,
            timeout_ms,
        }
    }
}

async fn respond(
    (behavior, queries): &(Behavior, Log),
    query: &[u8],
    tcp: bool,
) -> Option<Vec<u8>> {
    let query = Message::from_vec(query).unwrap();
    queries.lock().unwrap().push((query.clone(), tcp));
    if behavior.silent {
        return None;
    }
    tokio::time::sleep(behavior.delay).await;
    let mut reply = Message::response(query.metadata.id, OpCode::Query);
    reply.queries = query.queries.clone();
    reply.metadata.response_code = behavior.rcode.unwrap_or(ResponseCode::NoError);
    let name = query.queries[0].name().clone();
    if behavior.truncate_udp && !tcp {
        reply.metadata.truncation = true;
    } else if behavior.rcode.is_none() {
        let marker = Ipv4Addr::new(10, 0, 0, behavior.marker);
        reply
            .answers
            .push(Record::from_rdata(name.clone(), 60, RData::A(A(marker))));
        if behavior.big {
            for _ in 0..8 {
                let txt = TXT::new(vec!["x".repeat(100)]);
                reply
                    .answers
                    .push(Record::from_rdata(name.clone(), 60, RData::TXT(txt)));
            }
        }
    }
    let bytes = reply.to_vec().unwrap();
    if !tcp && bytes.len() > usize::from(query.max_payload()) {
        reply.answers.clear();
        reply.metadata.truncation = true;
        return Some(reply.to_vec().unwrap());
    }
    Some(bytes)
}

fn query(id: u16) -> Message {
    let mut query = Message::new(id, MessageType::Query, OpCode::Query);
    query.metadata.recursion_desired = true;
    query.add_query(Query::query(
        Name::from_str("example.com.").unwrap(),
        RecordType::A,
    ));
    query.edns = Some(Edns::new());
    query
}

fn forwarder(method: Method, interface: Option<&str>, servers: Vec<Server>) -> Forwarder {
    Forwarder::new(&Config {
        listen: "127.0.0.1:0".parse().unwrap(),
        interface: interface.map(String::from),
        method,
        servers,
    })
    .unwrap()
}

/// The marker address that tells which mock answered.
fn marker(reply: &Message) -> Option<u8> {
    reply.answers.iter().find_map(|record| match &record.data {
        RData::A(a) => Some(a.0.octets()[3]),
        _ => None,
    })
}

async fn ask(forwarder: &Forwarder, query: &Message, udp: bool) -> Message {
    let reply = forwarder
        .answer(&query.to_vec().unwrap(), udp)
        .await
        .unwrap();
    Message::from_vec(&reply).unwrap()
}

#[tokio::test]
async fn forwards_the_id_flags_and_edns_options_unchanged() {
    let mock = Mock::spawn(Behavior {
        marker: 1,
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(Method::InOrder, None, vec![mock.server(1000)]);
    let mut query = query(0x1234);
    query.metadata.checking_disabled = true;
    let edns = query.edns.as_mut().unwrap();
    edns.set_dnssec_ok(true);
    edns.options_mut()
        .insert(EdnsOption::Unknown(65001, b"kept".to_vec()));

    let reply = ask(&forwarder, &query, true).await;
    assert_eq!(reply.metadata.id, 0x1234);
    assert_eq!(marker(&reply), Some(1));

    let (sent, tcp) = mock.queries().pop().unwrap();
    assert!(!tcp);
    assert!(sent.metadata.checking_disabled);
    let edns = sent.edns.unwrap();
    assert!(edns.flags().dnssec_ok);
    assert_eq!(
        edns.option(EdnsCode::Unknown(65001)),
        Some(&EdnsOption::Unknown(65001, b"kept".to_vec()))
    );
}

#[tokio::test]
async fn in_order_skips_silent_and_failing_servers_and_stops_at_nxdomain() {
    let silent = Mock::spawn(Behavior {
        silent: true,
        ..Default::default()
    })
    .await;
    let servfail = Mock::spawn(Behavior {
        rcode: Some(ResponseCode::ServFail),
        ..Default::default()
    })
    .await;
    let nxdomain = Mock::spawn(Behavior {
        rcode: Some(ResponseCode::NXDomain),
        ..Default::default()
    })
    .await;
    let unused = Mock::spawn(Behavior {
        marker: 4,
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(
        Method::InOrder,
        None,
        vec![
            silent.server(200),
            servfail.server(1000),
            nxdomain.server(1000),
            unused.server(1000),
        ],
    );

    let started = std::time::Instant::now();
    let reply = ask(&forwarder, &query(7), true).await;
    assert_eq!(reply.metadata.response_code, ResponseCode::NXDomain);
    assert!(
        started.elapsed() >= Duration::from_millis(200),
        "waited for the silent server"
    );
    assert_eq!(silent.queries().len(), 1);
    assert_eq!(servfail.queries().len(), 1);
    assert!(unused.queries().is_empty());
}

#[tokio::test]
async fn random_reaches_every_server() {
    let mut mocks = Vec::new();
    for marker in 1..=3 {
        mocks.push(
            Mock::spawn(Behavior {
                marker,
                ..Default::default()
            })
            .await,
        );
    }
    let forwarder = forwarder(
        Method::Random,
        None,
        mocks.iter().map(|m| m.server(1000)).collect(),
    );
    for id in 0..60 {
        let reply = ask(&forwarder, &query(id), true).await;
        assert!(marker(&reply).is_some());
    }
    for mock in &mocks {
        assert!(!mock.queries().is_empty());
    }
    assert_eq!(mocks.iter().map(|m| m.queries().len()).sum::<usize>(), 60);
}

#[tokio::test]
async fn fastest_returns_the_quickest_final_answer() {
    let slow = Mock::spawn(Behavior {
        marker: 1,
        delay: Duration::from_millis(300),
        ..Default::default()
    })
    .await;
    let failing = Mock::spawn(Behavior {
        rcode: Some(ResponseCode::Refused),
        ..Default::default()
    })
    .await;
    let fast = Mock::spawn(Behavior {
        marker: 3,
        delay: Duration::from_millis(50),
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(
        Method::Fastest,
        None,
        vec![slow.server(1000), failing.server(1000), fast.server(1000)],
    );
    let started = std::time::Instant::now();
    let reply = ask(&forwarder, &query(9), true).await;
    assert_eq!(marker(&reply), Some(3));
    assert!(started.elapsed() < Duration::from_millis(300));
    for mock in [&slow, &failing, &fast] {
        assert_eq!(mock.queries().len(), 1);
    }
}

#[tokio::test]
async fn failing_servers_give_servfail_or_their_last_answer() {
    let silent = Mock::spawn(Behavior {
        silent: true,
        ..Default::default()
    })
    .await;
    let alone = forwarder(Method::InOrder, None, vec![silent.server(100)]);
    let mut dnssec = query(11);
    dnssec.edns.as_mut().unwrap().set_dnssec_ok(true);
    let reply = ask(&alone, &dnssec, true).await;
    assert_eq!(reply.metadata.id, 11);
    assert_eq!(reply.metadata.response_code, ResponseCode::ServFail);
    assert_eq!(reply.queries, dnssec.queries);
    assert!(reply.edns.unwrap().flags().dnssec_ok);
    let reply = ask(&alone, &query(10), true).await;
    assert!(!reply.edns.unwrap().flags().dnssec_ok);

    let refused = Mock::spawn(Behavior {
        rcode: Some(ResponseCode::Refused),
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(
        Method::Fastest,
        None,
        vec![silent.server(100), refused.server(1000)],
    );
    let reply = ask(&forwarder, &query(12), true).await;
    assert_eq!(reply.metadata.response_code, ResponseCode::Refused);
}

#[tokio::test]
async fn oversized_udp_answers_are_truncated_for_the_client() {
    let mock = Mock::spawn(Behavior {
        marker: 1,
        big: true,
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(Method::InOrder, None, vec![mock.server(1000)]);
    let mut small = query(13);
    small.edns = None;

    let reply = ask(&forwarder, &small, true).await;
    assert!(reply.metadata.truncation);
    assert!(reply.answers.is_empty());
    assert_eq!(reply.queries, small.queries);

    let reply = ask(&forwarder, &small, false).await;
    assert!(!reply.metadata.truncation);
    assert_eq!(marker(&reply), Some(1));

    let transports: Vec<_> = mock.queries().into_iter().map(|(_, tcp)| tcp).collect();
    assert_eq!(transports, [false, true, false, true]);
}

#[tokio::test]
async fn truncated_upstream_answers_are_retried_over_tcp() {
    let mock = Mock::spawn(Behavior {
        marker: 5,
        truncate_udp: true,
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(Method::InOrder, None, vec![mock.server(1000)]);
    let reply = ask(&forwarder, &query(14), true).await;
    assert_eq!(reply.metadata.id, 14);
    assert_eq!(marker(&reply), Some(5));
    let transports: Vec<_> = mock.queries().into_iter().map(|(_, tcp)| tcp).collect();
    assert_eq!(transports, [false, true]);
}

#[tokio::test]
async fn closed_connections_are_reopened_for_the_query_that_finds_them() {
    let mock = Mock::spawn(Behavior {
        marker: 7,
        truncate_udp: true,
        close_tcp: true,
        ..Default::default()
    })
    .await;
    let forwarder = forwarder(Method::InOrder, None, vec![mock.server(1000)]);
    for id in [17, 18] {
        assert_eq!(marker(&ask(&forwarder, &query(id), true).await), Some(7));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn sockets_are_bound_to_the_interface() {
    let mock = Mock::spawn(Behavior {
        marker: 6,
        truncate_udp: true,
        ..Default::default()
    })
    .await;
    let bound = forwarder(Method::InOrder, Some("lo"), vec![mock.server(1000)]);
    assert_eq!(marker(&ask(&bound, &query(15), true).await), Some(6));

    let before = mock.queries().len();
    let missing = forwarder(Method::InOrder, Some("nosuchdev0"), vec![mock.server(500)]);
    let reply = ask(&missing, &query(16), true).await;
    assert_eq!(reply.metadata.response_code, ResponseCode::ServFail);
    assert_eq!(mock.queries().len(), before, "nothing was sent");
}

#[tokio::test]
async fn serves_udp_and_tcp_clients() {
    let mock = Mock::spawn(Behavior {
        marker: 7,
        ..Default::default()
    })
    .await;
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let listen = udp.local_addr().unwrap();
    let tcp = TcpListener::bind(listen).await.unwrap();
    tokio::spawn(crate::serve(
        forwarder(Method::InOrder, None, vec![mock.server(1000)]),
        udp,
        tcp,
    ));

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client
        .send_to(&query(17).to_vec().unwrap(), listen)
        .await
        .unwrap();
    let mut buf = vec![0u8; 4096];
    let len = tokio::time::timeout(Duration::from_secs(2), client.recv(&mut buf))
        .await
        .unwrap()
        .unwrap();
    let reply = Message::from_vec(&buf[..len]).unwrap();
    assert_eq!((reply.metadata.id, marker(&reply)), (17, Some(7)));

    let mut stream = TcpStream::connect(listen).await.unwrap();
    for id in [18, 19] {
        let query = query(id).to_vec().unwrap();
        let len = u16::try_from(query.len()).unwrap().to_be_bytes();
        stream
            .write_all(&[&len, query.as_slice()].concat())
            .await
            .unwrap();
        let mut len = [0u8; 2];
        stream.read_exact(&mut len).await.unwrap();
        let mut reply = vec![0u8; usize::from(u16::from_be_bytes(len))];
        stream.read_exact(&mut reply).await.unwrap();
        let reply = Message::from_vec(&reply).unwrap();
        assert_eq!((reply.metadata.id, marker(&reply)), (id, Some(7)));
    }
}

#[tokio::test]
async fn undecodable_queries_get_formerr_and_responses_get_nothing() {
    let mock = Mock::spawn(Behavior::default()).await;
    let forwarder = forwarder(Method::InOrder, None, vec![mock.server(1000)]);
    let reply = forwarder.answer(&[0xAB, 0xCD, 0xFF], true).await.unwrap();
    let reply = Message::from_vec(&reply).unwrap();
    assert_eq!(reply.metadata.id, 0xABCD);
    assert_eq!(reply.metadata.response_code, ResponseCode::FormErr);

    let mut response = query(20);
    response.metadata.message_type = MessageType::Response;
    assert!(forwarder
        .answer(&response.to_vec().unwrap(), true)
        .await
        .is_none());
    assert!(forwarder.answer(&[0xAB], true).await.is_none());
    assert!(mock.queries().is_empty());
}
