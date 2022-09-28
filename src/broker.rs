use std::collections::VecDeque;

use async_std::sync::{Arc, Mutex};

use serde::{de::DeserializeOwned, Serialize};

mod mqtt_conn;
mod rest;
mod topic;

pub use mqtt_conn::TopicName;
use topic::RetainedValue;
pub use topic::{AnySubscriptionHandle, AnyTopic, Native, SubscriptionHandle, Topic};

pub struct BrokerBuilder {
    topics: Vec<Arc<dyn AnyTopic>>,
}

impl BrokerBuilder {
    pub fn new() -> Self {
        Self { topics: Vec::new() }
    }

    /// Register a new topic
    ///
    /// Please not that you can build topics that perform some kind of
    /// validation by registering a read only topic and a write only topic
    /// with the same path.
    /// This way your application can subscribe to events on the wo topic,
    /// process them and set the ro topic without any transient events on the
    /// topic containing an invalid state.
    ///
    /// # Arguments
    ///
    /// * `path` - Where to mount the MQTT topic and REST ressource
    /// * `web_readable` - Should this ressource be externally readable?
    /// * `web_writable` - Should this ressource be externally writable?
    /// * `initial` - Retained value to return before set() was called the
    ///    first time. Or None
    pub fn topic<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
        web_readable: bool,
        web_writable: bool,
        initial: Option<E>,
        retained_length: usize,
    ) -> Arc<Topic<E>> {
        let path = TopicName::new(path).unwrap();
        let retained = {
            let mut retained = VecDeque::with_capacity(retained_length + 1);

            if let Some(v) = initial {
                retained.push_back(RetainedValue::new(Arc::new(v)))
            }

            Mutex::new(retained)
        };

        let topic = Arc::new(Topic {
            path,
            web_readable,
            web_writable,
            senders: Mutex::new(Vec::new()),
            retained,
            retained_length,
            senders_serialized: Mutex::new(Vec::new()),
        });

        self.topics.push(topic.clone());

        topic
    }

    /// Register a new topic that is only readable from the outside
    pub fn topic_ro<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(path, true, false, initial, 1)
    }

    /// Register a new topic that is both readable and writable from the outside
    pub fn topic_rw<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(path, true, true, initial, 1)
    }

    /// Register a new topic that is only writable from the outside
    pub fn topic_wo<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        path: &str,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(path, false, true, initial, 1)
    }

    /// Register a new topic that can only be used internally
    pub fn topic_hidden<E: Serialize + DeserializeOwned + Sync + Send + 'static>(
        &mut self,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(&"/hidden", false, false, initial, 1)
    }

    /// Finish building the broker
    ///
    /// This consumes the builder so that no new topics can be registered
    pub fn build(self, server: &mut tide::Server<()>) {
        let topics = Arc::new(self.topics);

        rest::register(server, topics.clone());
        mqtt_conn::register(server, topics.clone());
    }
}
