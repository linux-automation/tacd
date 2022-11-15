use std::collections::HashMap;
use std::io::Cursor;

use async_std::channel::bounded;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::spawn;

use tide_websockets::{WebSocket, WebSocketConnection};

use mqtt::control::variable_header::{ConnectReturnCode, ProtocolLevel};
use mqtt::packet::publish::QoSWithPacketIdentifier;
use mqtt::packet::suback::SubscribeReturnCode;
use mqtt::TopicFilter;
use mqtt::{packet::*, Decodable, Encodable};

pub use mqtt::TopicName;

use super::{AnySubscriptionHandle, AnyTopic};

const MAX_QUEUE_LENGTH: usize = 256;

trait DecodableExt: Decodable
where
    <Self as Decodable>::Cond: Default,
{
    fn from_bytes(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Self::decode(&mut Cursor::new(bytes))
    }
}

impl<D> DecodableExt for D
where
    D: Decodable,
    <D as Decodable>::Cond: Default,
{
}

trait EncodableExt: Encodable {
    fn as_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        self.encode(&mut cursor)?;
        Ok(cursor.into_inner())
    }
}

impl<E> EncodableExt for E where E: Encodable {}

async fn handle_connection(
    topics: Arc<Vec<Arc<dyn AnyTopic>>>,
    mut stream: WebSocketConnection,
) -> tide::Result<()> {
    let conn_pkg = {
        let msg = stream
            .next()
            .await
            .ok_or_else(|| tide::Error::from_str(500, "Unexpected end of stream"))??
            .into_data();

        match VariablePacket::from_bytes(msg)? {
            VariablePacket::ConnectPacket(conn) => Ok(conn),
            _ => Err(tide::Error::from_str(
                500,
                "Protocol violation. Expected CONNECT",
            )),
        }?
    };

    if conn_pkg.user_name().is_some()
        || conn_pkg.password().is_some()
        || conn_pkg.will().is_some()
        || conn_pkg.will_retain()
        || conn_pkg.protocol_level() != ProtocolLevel::Version311
    {
        Err(tide::Error::from_str(
            500,
            "Client does not implement the expected MQTT subset",
        ))?
    }

    stream
        .send_bytes(ConnackPacket::new(false, ConnectReturnCode::ConnectionAccepted).as_bytes()?)
        .await?;

    let mut subscription_handles: HashMap<TopicFilter, Vec<Box<dyn AnySubscriptionHandle>>> =
        HashMap::new();

    let (to_websocket, mut for_websocket) = bounded::<(TopicName, Arc<[u8]>)>(MAX_QUEUE_LENGTH);
    let stream_tx = stream.clone();
    spawn(async move {
        while let Some((topic, payload)) = for_websocket.next().await {
            let pkg = PublishPacket::new(topic, QoSWithPacketIdentifier::Level0, payload.to_vec());

            if let Err(_) = stream_tx.send_bytes(pkg.as_bytes().unwrap()).await {
                break;
            }
        }
    });

    'connection: while let Some(pkg) = stream
        .next()
        .await
        .transpose()
        .ok()
        .flatten()
        .map(|msg| VariablePacket::from_bytes(msg.into_data()).ok())
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
                .as_bytes()
                .unwrap();

                if stream.send_bytes(suback_pkg).await.is_err() {
                    break 'connection;
                }

                for (filter, _qos) in sub_pkg.subscribes() {
                    let matcher = filter.get_matcher();
                    let sub_topics = topics
                        .iter()
                        .filter(|topic| topic.web_readable() && matcher.is_match(&topic.path()));

                    let mut new_subscribes = Vec::new();

                    for topic in sub_topics {
                        if let Some(retained) = topic.get_as_bytes().await {
                            // Handle full?
                            let _ = to_websocket.try_send((topic.path().clone(), retained));
                        }

                        let sub_handle =
                            topic.clone().subscribe_as_bytes(to_websocket.clone()).await;

                        new_subscribes.push(sub_handle);
                    }

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
                    .as_bytes()
                    .unwrap();

                if stream.send_bytes(unsuback_pkg).await.is_err() {
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
                let pingresp_pkg = PingrespPacket::new().as_bytes().unwrap();

                if stream.send_bytes(pingresp_pkg).await.is_err() {
                    break 'connection;
                }
            }
            _ => break 'connection,
        }
    }

    for desub in subscription_handles.into_values().flatten() {
        desub.unsubscribe().await
    }

    Ok(())
}

pub(super) fn register(server: &mut tide::Server<()>, topics: Arc<Vec<Arc<dyn AnyTopic>>>) {
    server
        .at("/v1/mqtt")
        .get(WebSocket::new(move |_request, stream| {
            handle_connection(topics.clone(), stream)
        }));
}
