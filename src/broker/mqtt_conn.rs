use std::collections::HashMap;
use std::io::Cursor;

use async_std::channel::bounded;
use async_std::sync::{Arc, Mutex};
use async_std::task::spawn;

use async_tungstenite::tungstenite::{protocol::Role, Message};
use async_tungstenite::WebSocketStream;

use futures_util::{SinkExt, StreamExt};

use mqtt::control::variable_header::{ConnectReturnCode, ProtocolLevel};
use mqtt::packet::publish::QoSWithPacketIdentifier;
use mqtt::packet::suback::SubscribeReturnCode;
use mqtt::TopicFilter;
use mqtt::{packet::*, Decodable, Encodable};

use sha1::{Digest, Sha1};

use tide::http::format_err;
use tide::http::headers::{HeaderName, CONNECTION, UPGRADE};
use tide::http::upgrade::Connection;
use tide::{Request, Response, StatusCode};

pub use mqtt::TopicName;

use super::{AnySubscriptionHandle, AnyTopic};

/// Limit the number of elements in the queue leading to the websocket
/// connection. This assumes that the websocket connection will provide
/// backpressure when overloaded.
/// The intent is to drop the connection when overloaded so that the user
/// gets a visual indication that the web interface is no longer up to date.
const MAX_QUEUE_LENGTH: usize = 256;

/// Force a flush on the Websocket every now and then to make sure that
/// the backpressure mechanism mentioned above actually does something.
const MAX_PENDING_BYTES: usize = 256 * 1024;

/// This is used in the WebSocket handshake
const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

// The mqtt crate provides the Decodable and Encodable traits that can decode/
// encode packets from/to Readers/Writers.
// This is nice, but we use WebSocket Messages instead of Readers/Writers.
// Provide convenience wrappers that use/provide Messages directly.
trait DecodableExt: Decodable
where
    <Self as Decodable>::Cond: Default,
{
    fn from_message(msg: Message) -> Result<Self, Self::Error> {
        Self::decode(&mut Cursor::new(msg.into_data()))
    }
}

impl<D> DecodableExt for D
where
    D: Decodable,
    <D as Decodable>::Cond: Default,
{
}

trait EncodableExt: Encodable {
    fn as_message(&self) -> std::io::Result<Message> {
        let mut cursor = Cursor::new(Vec::new());
        self.encode(&mut cursor)?;
        Ok(Message::binary(cursor.into_inner()))
    }
}

impl<E> EncodableExt for E where E: Encodable {}

/// Handle the full lifetime of a MQTT over websocket connection,
/// from protocol handshake to teardown.
async fn handle_connection(
    topics: Arc<Vec<Arc<dyn AnyTopic>>>,
    mut stream: WebSocketStream<Connection>,
) {
    // The MQTT connection starts with a CONNECT packet.
    // Since we are only targeting the one MQTT (over WebSockets)
    // implementation used in the web interface we can make some assumptions.
    // The first one is that MQTT packets will always be aligned with
    // Websocket frames. If we would want to support MQTT over raw TCP as well
    // we would have to use the length fields contained in the MQTT packets.
    let conn_pkg = {
        let msg = match stream.next().await {
            Some(Ok(msg)) => msg,
            _ => return,
        };

        match VariablePacket::from_message(msg) {
            Ok(VariablePacket::ConnectPacket(conn)) => conn,
            _ => return,
        }
    };

    // The second assumption is that the client will always use the same MQTT
    // subset. If a client comes around and wants to use features we do not
    // know we can simply drop the connection.
    if conn_pkg.user_name().is_some()
        || conn_pkg.password().is_some()
        || conn_pkg.will().is_some()
        || conn_pkg.will_retain()
        || conn_pkg.protocol_level() != ProtocolLevel::Version311
    {
        return;
    }

    // Send CONNACK packet to signal a successful connection setup
    if stream
        .send(
            ConnackPacket::new(false, ConnectReturnCode::ConnectionAccepted)
                .as_message()
                .unwrap(),
        )
        .await
        .is_err()
    {
        return;
    }

    if stream.flush().await.is_err() {
        return;
    }

    let (stream_tx, mut stream_rx) = stream.split();
    let stream_tx = Arc::new(Mutex::new(stream_tx));

    // Set up a task that takes messages from a queue, wraps them in a MQTT
    // packet and sends them out over the websocket.
    // This should generate backpressure on the queue if the websocket can not
    // make progress and the senders should close the queue if it is full.
    let (to_websocket, mut for_websocket) = bounded::<(TopicName, Arc<[u8]>)>(MAX_QUEUE_LENGTH);
    let stream_tx_task = stream_tx.clone();
    spawn(async move {
        let mut pending_bytes = 0;

        while let Some((topic, payload)) = for_websocket.next().await {
            let pkg = PublishPacket::new(topic, QoSWithPacketIdentifier::Level0, payload.to_vec());

            let msg = match pkg.as_message() {
                Ok(msg) => msg,
                Err(_) => break,
            };

            pending_bytes += msg.len();

            // Enqueue the message for sending
            if stream_tx_task.lock().await.send(msg).await.is_err() {
                break;
            }

            // Make sure that every now and then the messages are actually sent out
            if pending_bytes > MAX_PENDING_BYTES {
                if stream_tx_task.lock().await.flush().await.is_err() {
                    break;
                }
                pending_bytes = 0;
            }
        }

        // Something went wrong. Make sure that the client is informed about it.
        stream_tx_task.lock().await.close().await
    });

    // Keep track of the currently subscribed topics to be able to handle
    // unsubscribe requests and clean up once the connection is closed.
    let mut subscription_handles: HashMap<TopicFilter, Vec<Box<dyn AnySubscriptionHandle>>> =
        HashMap::new();

    // Handle packets sent by the client
    'connection: while let Some(pkg) = stream_rx
        .next()
        .await
        .transpose()
        .ok()
        .flatten()
        .map(|msg| VariablePacket::from_message(msg).ok())
        .flatten()
    {
        match pkg {
            VariablePacket::SubscribePacket(sub_pkg) => {
                let suback_pkg = SubackPacket::new(
                    sub_pkg.packet_identifier(),
                    sub_pkg
                        .subscribes()
                        .iter()
                        .map(|_| SubscribeReturnCode::MaximumQoSLevel0)
                        .collect(),
                )
                .as_message()
                .unwrap();

                // We should get the suback out before sending the retained
                // values. So send it now even though we did not do the
                // subscribing yet.
                if stream_tx.lock().await.send(suback_pkg).await.is_err() {
                    break 'connection;
                }

                // One subscribe packet can (in theory) contain multiple topics
                // (including wildcards) to subscribe to.
                // Currently the web interface uses neither of these features,
                // but it could.
                for (filter, _qos) in sub_pkg.subscribes() {
                    // Go through all registered topics and check if the
                    // subscribe request matches. This should make sure that
                    // wildcard subscriptions work.
                    let matcher = filter.get_matcher();
                    let sub_topics = topics
                        .iter()
                        .filter(|topic| topic.web_readable() && matcher.is_match(&topic.path()));

                    let mut new_subscribes = Vec::new();

                    for topic in sub_topics {
                        // Do we have a retained value for this topic?
                        // If so: send it to the client
                        if let Some(retained) = topic.try_get_as_bytes().await {
                            if let Err(_) = to_websocket.try_send((topic.path().clone(), retained))
                            {
                                break 'connection;
                            }
                        }

                        // Subscribe to the serialized messages via the broker
                        // framwork. This uses a single queue per connection for
                        // all topics.
                        let sub_handle =
                            topic.clone().subscribe_as_bytes(to_websocket.clone()).await;

                        new_subscribes.push(sub_handle);
                    }

                    // Only allow one subscribe with the same match per
                    // connection, so if there is an existing one it should
                    // be cleared.
                    if let Some(old_subscribes) =
                        subscription_handles.insert(filter.clone(), new_subscribes)
                    {
                        for unsub in old_subscribes {
                            unsub.unsubscribe().await
                        }
                    }
                }
            }
            VariablePacket::UnsubscribePacket(unsub_pkg) => {
                for filter in unsub_pkg.subscribes() {
                    if let Some(old_subscribes) = subscription_handles.remove(filter) {
                        for unsub in old_subscribes {
                            unsub.unsubscribe().await
                        }
                    }
                }

                let unsuback_pkg = UnsubackPacket::new(unsub_pkg.packet_identifier())
                    .as_message()
                    .unwrap();

                if stream_tx.lock().await.send(unsuback_pkg).await.is_err() {
                    break 'connection;
                }
            }
            VariablePacket::PublishPacket(pub_pkg) => {
                if pub_pkg.qos() != QoSWithPacketIdentifier::Level0
                    || pub_pkg.dup() != false
                    || pub_pkg.retain() != true
                {
                    break 'connection;
                }

                let topic = topics
                    .iter()
                    .filter(|t| t.web_writable() && &t.path()[..] == pub_pkg.topic_name())
                    .next();

                if let Some(topic) = topic {
                    if let Err(_) = topic.set_from_bytes(pub_pkg.payload()).await {
                        break 'connection;
                    }
                }
            }
            VariablePacket::PingreqPacket(_) => {
                let pingresp_pkg = PingrespPacket::new().as_message().unwrap();

                if stream_tx.lock().await.send(pingresp_pkg).await.is_err() {
                    break 'connection;
                }
            }
            _ => break 'connection,
        }
    }

    for desub in subscription_handles.into_values().flatten() {
        desub.unsubscribe().await
    }
}

fn header_contains_ignore_case(req: &Request<()>, header_name: HeaderName, value: &str) -> bool {
    req.header(header_name)
        .map(|h| {
            h.as_str()
                .split(',')
                .any(|s| s.trim().eq_ignore_ascii_case(value.trim()))
        })
        .unwrap_or(false)
}

pub(super) fn register(server: &mut tide::Server<()>, topics: Arc<Vec<Arc<dyn AnyTopic>>>) {
    server.at("/v1/mqtt").get(move |req: Request<()>| {
        let topics = topics.clone();

        async move {
            // These are the good parts from tide-websockets without the bad
            // WebSocketConnection wrapper.

            let connection_upgrade = header_contains_ignore_case(&req, CONNECTION, "upgrade");
            let upgrade_to_websocket = header_contains_ignore_case(&req, UPGRADE, "websocket");
            let upgrade_requested = connection_upgrade && upgrade_to_websocket;

            if !upgrade_requested {
                return Ok(Response::new(StatusCode::UpgradeRequired));
            }

            let header = match req.header("Sec-Websocket-Key") {
                Some(h) => h.as_str(),
                None => return Err(format_err!("expected sec-websocket-key")),
            };

            let protocol = req.header("Sec-Websocket-Protocol").and_then(|value| {
                value
                    .as_str()
                    .split(',')
                    .map(str::trim)
                    .find(|req_p| req_p == &"mqttv3.1" || req_p == &"mqtt")
            });

            let mut response = Response::new(StatusCode::SwitchingProtocols);

            response.insert_header(UPGRADE, "websocket");
            response.insert_header(CONNECTION, "Upgrade");
            let hash = Sha1::new().chain(header).chain(WEBSOCKET_GUID).finalize();
            response.insert_header("Sec-Websocket-Accept", base64::encode(&hash[..]));
            response.insert_header("Sec-Websocket-Version", "13");

            if let Some(protocol) = protocol {
                response.insert_header("Sec-Websocket-Protocol", protocol);
            }

            let http_res: &mut tide::http::Response = response.as_mut();
            let upgrade_receiver = http_res.recv_upgrade().await;

            spawn(async move {
                if let Some(stream) = upgrade_receiver.await {
                    let ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
                    handle_connection(topics, ws).await;
                }
            });

            Ok(response)
        }
    });
}
