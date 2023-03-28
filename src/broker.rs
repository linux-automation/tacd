// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2022 Pengutronix e.K.
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use async_std::sync::Arc;
use serde::{de::DeserializeOwned, Serialize};

mod mqtt_conn;
mod persistence;
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

    /// Register a new topic
    ///
    /// Please note that you can build topics that perform some kind of
    /// validation by registering a read only topic and a write only topic
    /// with the same path.
    /// This way your application can subscribe to events on the wo topic,
    /// process them and set the ro topic without any transient events on the
    /// topic containing an invalid state.
    ///
    /// # Arguments
    ///
    /// * `path` - Where to mount the MQTT topic and REST resource
    /// * `web_readable` - Should this resource be externally readable?
    /// * `web_writable` - Should this resource be externally writable?
    /// * `initial` - Retained value to return before set() was called the
    ///    first time. Or None
    /// * `retained_length` - Number of previously set values to retain
    ///    and push out when subscribing to the serialized stream.
    ///    This will usually be 1 so that the most recent value is available
    ///    when doing a GET request to the topic via the REST API.
    ///    It can also be 0 for topics that are purely transient events like
    ///    button presses that go away as soon as they happen.
    ///    It can also be a larger value to store up some history that should
    ///    be pushed out to new (outside) subscribers as soon as they subscribe,
    ///    to e.g. pre-populate a graph in the web interface.
    pub fn topic<E: Serialize + DeserializeOwned + Sync + Send + Clone + 'static>(
        &mut self,
        path: &str,
        web_readable: bool,
        web_writable: bool,
        persistent: bool,
        initial: Option<E>,
        retained_length: usize,
    ) -> Arc<Topic<E>> {
        let topic = Arc::new(Topic::new(
            path,
            web_readable,
            web_writable,
            persistent,
            initial,
            retained_length,
        ));

        self.topics.push(topic.clone());

        topic
    }

    /// Register a new topic that is only readable from the outside
    pub fn topic_ro<E: Serialize + DeserializeOwned + Sync + Send + Clone + 'static>(
        &mut self,
        path: &str,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(path, true, false, false, initial, 1)
    }

    /// Register a new topic that is both readable and writable from the outside
    pub fn topic_rw<E: Serialize + DeserializeOwned + Sync + Send + Clone + 'static>(
        &mut self,
        path: &str,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(path, true, true, false, initial, 1)
    }

    /// Register a new topic that is only writable from the outside
    pub fn topic_wo<E: Serialize + DeserializeOwned + Sync + Send + Clone + 'static>(
        &mut self,
        path: &str,
        initial: Option<E>,
    ) -> Arc<Topic<E>> {
        self.topic(path, false, true, false, initial, 1)
    }

    /// Finish building the broker
    ///
    /// This consumes the builder so that no new topics can be registered
    pub fn build(self, server: &mut tide::Server<()>) {
        let topics = Arc::new(self.topics);

        persistence::register(topics.clone());
        rest::register(server, topics.clone());
        mqtt_conn::register(server, topics);
    }
}
