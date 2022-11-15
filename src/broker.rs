use async_std::sync::{Arc, Mutex};

use serde::{de::DeserializeOwned, Serialize};

mod mqtt_conn;
mod rest;
mod topic;

pub use mqtt_conn::TopicName;
pub use topic::{AnySubscriptionHandle, AnyTopic, Native, SubscriptionHandle, Topic};

pub struct BrokerBuilder {
    topics: Vec<Arc<dyn AnyTopic>>,
}

impl BrokerBuilder {
    pub fn new() -> Self {
        Self { topics: Vec::new() }
    }

    pub fn topic<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
        web_readable: bool,
        web_writable: bool,
    ) -> Arc<Topic<E>> {
        let path = TopicName::new(path).unwrap();

        let topic = Arc::new(Topic {
            path,
            web_readable,
            web_writable,
            senders: Mutex::new(Vec::new()),
            retained: Mutex::new(None),
            senders_serialized: Mutex::new(Vec::new()),
            retained_serialized: Mutex::new(None),
        });

        self.topics.push(topic.clone());

        topic
    }

    pub fn topic_ro<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
    ) -> Arc<Topic<E>> {
        self.topic(path, true, false)
    }

    pub fn topic_rw<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
    ) -> Arc<Topic<E>> {
        self.topic(path, true, true)
    }

    pub fn topic_wo<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
    ) -> Arc<Topic<E>> {
        self.topic(path, false, true)
    }

    pub fn topic_hidden<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
    ) -> Arc<Topic<E>> {
        self.topic(&"/hidden", false, false)
    }

    pub fn build(self, server: &mut tide::Server<()>) {
        let topics = Arc::new(self.topics);

        rest::register(server, topics.clone());
        mqtt_conn::register(server, topics.clone());
    }
}
