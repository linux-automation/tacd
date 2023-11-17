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

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::ops::Not;
use std::sync::{Arc, Mutex, Weak};

use async_std::channel::{unbounded, Receiver, Sender, TrySendError};
use async_std::prelude::*;

use serde::{de::DeserializeOwned, Serialize};

use unique_token::Unique;

use super::TopicName;

pub(super) struct RetainedValue<E> {
    native: E,
    serialized: Option<Arc<[u8]>>,
}

impl<E: Serialize + Clone> RetainedValue<E> {
    pub(super) fn new(val: E) -> Self {
        Self {
            native: val,
            serialized: None,
        }
    }

    fn native(&self) -> E {
        self.native.clone()
    }

    /// Get the contained value serialized as json
    ///
    /// Returns either a cached result or serializes the value and caches it
    /// for later.
    fn serialized(&mut self) -> Arc<[u8]> {
        let native = &self.native;

        self.serialized
            .get_or_insert_with(|| {
                let ser = serde_json::to_vec(native).unwrap();
                Arc::from(ser.into_boxed_slice())
            })
            .clone()
    }
}

type SerializedSender = Sender<(TopicName, Arc<[u8]>)>;

pub struct TopicInner<E> {
    retained: VecDeque<RetainedValue<E>>,
    senders: Vec<(Unique, Sender<E>)>,
    senders_serialized: Vec<(Unique, SerializedSender)>,
}

impl<E: Serialize + Clone> TopicInner<E> {
    fn new(retained_length: usize, initial: Option<E>) -> Self {
        let mut retained = VecDeque::with_capacity(retained_length + 1);

        if let Some(v) = initial {
            retained.push_back(RetainedValue::new(v))
        }

        Self {
            retained,
            senders: Vec::new(),
            senders_serialized: Vec::new(),
        }
    }
}

pub struct Topic<E> {
    path: TopicName,
    web_readable: bool,
    web_writable: bool,
    persistent: bool,
    retained_length: usize,
    inner: Mutex<TopicInner<E>>,
}

pub struct Native;
pub struct Serialized;

pub struct SubscriptionHandle<E, T> {
    topic: Weak<Topic<E>>,
    token: Unique,
    phantom: PhantomData<T>,
}

impl<E> SubscriptionHandle<E, Native> {
    /// Unsubscribe a sender from the topic values
    ///
    /// The sender may already have been unsubscribed if e.g. the receiving side
    /// was dropped and set() was called. This will not result in an error.
    pub fn unsubscribe(self) {
        if let Some(topic) = self.topic.upgrade() {
            let mut inner = topic.inner.lock().unwrap();

            if let Some(idx) = inner
                .senders
                .iter()
                .position(|(token, _)| *token == self.token)
            {
                inner.senders.swap_remove(idx);
            }
        }
    }
}

pub trait AnySubscriptionHandle: Sync + Send {
    fn unsubscribe(&self);
}

impl<E: Send + Sync> AnySubscriptionHandle for SubscriptionHandle<E, Serialized> {
    /// Unsubscribe a sender from the serialized topic values
    ///
    /// The sender may already have been unsubscribed if e.g. the receiving side
    /// was dropped and set() was called. This will not result in an error.
    fn unsubscribe(&self) {
        if let Some(topic) = self.topic.upgrade() {
            let mut inner = topic.inner.lock().unwrap();

            if let Some(idx) = inner
                .senders_serialized
                .iter()
                .position(|(token, _)| *token == self.token)
            {
                inner.senders_serialized.swap_remove(idx);
            }
        }
    }
}

impl<E: Serialize + DeserializeOwned + Clone> Topic<E> {
    pub(super) fn new(
        path: &str,
        web_readable: bool,
        web_writable: bool,
        persistent: bool,
        initial: Option<E>,
        retained_length: usize,
    ) -> Self {
        let path = TopicName::new(path).unwrap();
        let inner = TopicInner::new(retained_length, initial);
        let inner = Mutex::new(inner);

        Self {
            path,
            web_readable,
            web_writable,
            persistent,
            retained_length,
            inner,
        }
    }

    pub fn anonymous(initial: Option<E>) -> Arc<Self> {
        Arc::new(Self::new("/hidden", false, false, false, initial, 1))
    }

    /// Set a new value for the topic and notify subscribers with the inner
    /// lock held to allow atomic read-modify-write cycles.
    ///
    /// # Arguments
    ///
    /// * `msg` - Value to set the topic to
    /// * `inner` - Locked mutable reference to the mutable parts of the
    ///   Topic struct.
    fn set_with_lock(&self, msg: E, inner: &mut TopicInner<E>) {
        let mut val = RetainedValue::new(msg);

        // Iterate through all native senders and try to enqueue the message.
        // In case of success keep the sender, if the (bounded) queue is full
        // close the queue (so that e.g. websockets are closed in the respective
        // task) and remove the sender from the list, if the queue is already
        // closed also remove it.
        inner
            .senders
            .retain(|(_, s)| match s.try_send(val.native()) {
                Ok(_) => true,
                Err(TrySendError::Full(_)) => {
                    s.close();
                    false
                }
                Err(TrySendError::Closed(_)) => false,
            });

        // Iterate through all serialized senders and do as above
        inner.senders_serialized.retain(|(_, s)| {
            match s.try_send((self.path.clone(), val.serialized())) {
                Ok(_) => true,
                Err(TrySendError::Full(_)) => {
                    s.close();
                    false
                }
                Err(TrySendError::Closed(_)) => false,
            }
        });

        inner.retained.push_back(val);

        while inner.retained.len() > self.retained_length {
            inner.retained.pop_front();
        }
    }

    /// Set a new value for the topic and notify subscribers
    ///
    /// # Arguments
    ///
    /// * `msg` - Value to set the topic to
    pub fn set(&self, msg: E) {
        let mut inner = self.inner.lock().unwrap();
        self.set_with_lock(msg, &mut *inner)
    }

    /// Get the current value
    ///
    /// Or nothing if none is set
    pub fn try_get(&self) -> Option<E> {
        self.inner
            .lock()
            .unwrap()
            .retained
            .back()
            .map(|v| v.native())
    }

    // Get the value of this topic
    //
    // Waits for a value if none was set yet
    pub async fn get(self: &Arc<Self>) -> E {
        let (mut rx, sub) = self.clone().subscribe_unbounded();
        let val = rx.next().await;
        sub.unsubscribe();

        // Unwrap here to keep the interface simple. The stream could only yield
        // None if the sender side is dropped, which will not happen as we hold
        // an Arc to self which contains the senders vec.
        val.unwrap()
    }

    /// Perform an atomic read modify write cycle for this topic
    ///
    /// The closure is called with the current value of the topic (may be None).
    /// If the value returned by the closure is Some(v) the value will then be
    /// set to v.
    pub fn modify<F>(&self, cb: F)
    where
        F: FnOnce(Option<E>) -> Option<E>,
    {
        let mut inner = self.inner.lock().unwrap();
        let retained = inner.retained.back().map(|v| v.native());

        if let Some(new) = cb(retained) {
            self.set_with_lock(new, &mut *inner);
        }
    }

    /// Add the provided sender to the list of subscribers
    ///
    /// The returned SubscriptionHandle can be used to remove the sender again
    /// from the list of subscribers. The subscriber will also be removed
    /// implicitly on the first `set` call after the receiving end of the queue
    /// was dropped.
    /// If a retained value is present it will be enqueued immediately.
    ///
    /// # Arguments
    ///
    /// * `sender` - The sender side of the queue to subscribe
    pub fn subscribe(self: Arc<Self>, sender: Sender<E>) -> SubscriptionHandle<E, Native> {
        let mut inner = self.inner.lock().unwrap();
        let token = Unique::new();

        // If there is a retained value try to enqueue it right away.
        // It that fails mimic what set_with_retain_lock would do.
        let retained_send_res = inner
            .retained
            .back()
            .map(|val| sender.try_send(val.native()))
            .unwrap_or(Ok(()));

        match retained_send_res {
            Ok(_) => {
                inner.senders.push((token, sender));
            }
            Err(TrySendError::Full(_)) => {
                sender.close();
            }
            Err(TrySendError::Closed(_)) => {}
        };

        SubscriptionHandle {
            topic: Arc::downgrade(&self),
            token,
            phantom: PhantomData,
        }
    }

    /// Create a new unbounded queue and subscribe it to the topic
    ///
    /// The returned SubscriptionHandle can be used to remove the sender again
    /// from the list of subscribers.
    /// If a retained value is present it will be enqueued immediately.
    pub fn subscribe_unbounded(self: Arc<Self>) -> (Receiver<E>, SubscriptionHandle<E, Native>) {
        let (tx, rx) = unbounded();
        (rx, self.subscribe(tx))
    }
}

impl<E: Serialize + DeserializeOwned + Clone + PartialEq> Topic<E> {
    /// Set a new value for the topic and notify subscribers _if the value changed_
    ///
    /// # Arguments
    ///
    /// * `msg` - Value to set the topic to
    pub fn set_if_changed(&self, msg: E) {
        let msg = Some(msg);

        self.modify(|prev| if prev != msg { msg } else { None });
    }

    /// Wait until the topic is set to the specified value
    #[allow(dead_code)]
    pub async fn wait_for(self: &Arc<Self>, val: E) {
        let (mut stream, sub) = self.clone().subscribe_unbounded();

        // Unwrap here to keep the interface simple. The stream could only yield
        // None if the sender side is dropped, which will not happen as we hold
        // an Arc to self which contains the senders vec.
        while stream.next().await.unwrap() != val {}

        sub.unsubscribe()
    }
}

impl<E: Serialize + DeserializeOwned + Clone + Not + Not<Output = E>> Topic<E> {
    /// Toggle the value of a topic
    ///
    /// # Arguments
    ///
    /// * `default` - The value to assume if none was set yet
    pub fn toggle(&self, default: E) {
        self.modify(|prev| Some(!prev.unwrap_or(default)));
    }
}

pub trait AnyTopic: Sync + Send {
    fn path(&self) -> &TopicName;
    fn web_readable(&self) -> bool;
    fn web_writable(&self) -> bool;
    fn persistent(&self) -> bool;
    fn set_from_bytes(&self, msg: &[u8]) -> serde_json::Result<()>;
    fn set_from_json_value(&self, msg: serde_json::Value) -> serde_json::Result<()>;
    fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
        enqueue_retained: bool,
    ) -> Box<dyn AnySubscriptionHandle>;
    fn try_get_as_bytes(&self) -> Option<Arc<[u8]>>;
    fn try_get_json_value(&self) -> Option<serde_json::Value>;
}

impl<E: Serialize + DeserializeOwned + Send + Sync + Clone + 'static> AnyTopic for Topic<E> {
    fn path(&self) -> &TopicName {
        &self.path
    }

    fn web_readable(&self) -> bool {
        self.web_readable
    }

    fn web_writable(&self) -> bool {
        self.web_writable
    }

    fn persistent(&self) -> bool {
        self.persistent
    }

    /// De-Serialize a message and set the topic to the resulting value
    ///
    /// Returns an Err if deserialization failed.
    fn set_from_bytes(&self, msg: &[u8]) -> serde_json::Result<()> {
        let msg = serde_json::from_slice(msg)?;
        self.set(msg);
        Ok(())
    }

    /// Take a value that was deserialized as serde_json value and set the
    /// topic to it.
    ///
    /// Returns an Err if de-structuring the generic value into this specific
    /// type failed.
    fn set_from_json_value(&self, msg: serde_json::Value) -> serde_json::Result<()> {
        let msg = serde_json::from_value(msg)?;
        self.set(msg);
        Ok(())
    }

    /// Add a queue to the list of subscribers for serialized values
    ///
    /// The Returned AnySubscriptionHandle can be used to remove the queue
    /// again from the list of subscribers.
    /// If retained values are present they will be enqueued immediately.
    ///
    /// # Arguments:
    ///
    /// * `sender` - The sender side of the queue to add
    /// * `enqueue_retained` - whether to enqueue the currently retained values
    fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
        enqueue_retained: bool,
    ) -> Box<dyn AnySubscriptionHandle> {
        let mut inner = self.inner.lock().unwrap();
        let token = Unique::new();
        let mut should_add = true;

        if enqueue_retained {
            // If there are retained values try to enqueue them right away.
            // It that fails mimic what set_arc_with_retain_lock would do.
            for val in inner.retained.iter_mut() {
                match sender.try_send((self.path.clone(), val.serialized())) {
                    Ok(_) => {}
                    Err(TrySendError::Full(_)) => {
                        sender.close();
                        should_add = false;
                        break;
                    }
                    Err(TrySendError::Closed(_)) => {
                        should_add = false;
                        break;
                    }
                }
            }
        }

        if should_add {
            inner.senders_serialized.push((token, sender));
        }

        let handle = SubscriptionHandle {
            topic: Arc::downgrade(&self),
            token,
            phantom: PhantomData,
        };

        Box::new(handle)
    }

    /// Try to get the current serialized topic value
    ///
    /// Returns None if no value was set yet.
    fn try_get_as_bytes(&self) -> Option<Arc<[u8]>> {
        self.inner
            .lock()
            .unwrap()
            .retained
            .back_mut()
            .map(|v| v.serialized())
    }

    /// Try to get the current value as serde_json value
    ///
    /// Returns None if no value was set yet.
    fn try_get_json_value(&self) -> Option<serde_json::Value> {
        self.inner
            .lock()
            .unwrap()
            .retained
            .back()
            .map(|v| serde_json::to_value(v.native()).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::{AnyTopic, RetainedValue, Topic, TopicName};
    use async_std::channel::{unbounded, Receiver};
    use async_std::sync::Arc;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct SerTestType {
        a: bool,
        b: u32,
        c: String,
    }

    fn new_topic<E: Serialize + DeserializeOwned + Clone>() -> Arc<Topic<E>> {
        Arc::new(Topic::new("/", true, true, true, None, 1))
    }

    fn collect_native<E: Clone>(recv: Receiver<E>) -> Vec<E> {
        std::iter::from_fn(|| recv.try_recv().ok()).collect()
    }

    fn collect_serialized(recv: Receiver<(TopicName, Arc<[u8]>)>) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| recv.try_recv().ok().map(|(_, v)| v.to_vec())).collect()
    }

    #[test]
    fn retained_is_cached() {
        let mut retained = RetainedValue::new(Arc::new(1u32));

        assert!(Arc::ptr_eq(&retained.native(), &retained.native()));
        assert!(Arc::ptr_eq(&retained.serialized(), &retained.serialized()));

        assert_eq!(&*retained.serialized(), &b"1"[..]);
    }

    #[test]
    fn unsubscribe_works() {
        let topic = new_topic::<u32>();

        let (native_1, native_handle_1) = topic.clone().subscribe_unbounded();
        let (native_2, native_handle_2) = topic.clone().subscribe_unbounded();
        let (native_3, native_handle_3) = topic.clone().subscribe_unbounded();

        let (ser_1, ser_handle_1) = {
            let (tx, rx) = unbounded();
            (rx, topic.clone().subscribe_as_bytes(tx, true))
        };

        let (ser_2, ser_handle_2) = {
            let (tx, rx) = unbounded();
            (rx, topic.clone().subscribe_as_bytes(tx, true))
        };

        let (ser_3, ser_handle_3) = {
            let (tx, rx) = unbounded();
            (rx, topic.clone().subscribe_as_bytes(tx, true))
        };

        assert_eq!(topic.inner.lock().unwrap().senders.len(), 3);
        assert_eq!(topic.inner.lock().unwrap().senders_serialized.len(), 3);

        topic.set(2);
        native_handle_2.unsubscribe();
        ser_handle_2.unsubscribe();

        assert_eq!(topic.inner.lock().unwrap().senders.len(), 2);
        assert_eq!(topic.inner.lock().unwrap().senders_serialized.len(), 2);

        topic.set(1);
        native_handle_1.unsubscribe();
        ser_handle_1.unsubscribe();

        assert_eq!(topic.inner.lock().unwrap().senders.len(), 1);
        assert_eq!(topic.inner.lock().unwrap().senders_serialized.len(), 1);

        topic.set(3);
        native_handle_3.unsubscribe();
        ser_handle_3.unsubscribe();

        assert_eq!(topic.inner.lock().unwrap().senders.len(), 0);
        assert_eq!(topic.inner.lock().unwrap().senders_serialized.len(), 0);

        topic.set(4);

        let native_1 = collect_native(native_1);
        let native_2 = collect_native(native_2);
        let native_3 = collect_native(native_3);

        let ser_1 = collect_serialized(ser_1);
        let ser_2 = collect_serialized(ser_2);
        let ser_3 = collect_serialized(ser_3);

        assert_eq!(&native_1, &[2, 1]);
        assert_eq!(&native_2, &[2]);
        assert_eq!(&native_3, &[2, 1, 3]);

        assert_eq!(&ser_1, &[b"2", b"1"]);
        assert_eq!(&ser_2, &[b"2"]);
        assert_eq!(&ser_3, &[b"2", b"1", b"3"]);
    }

    #[test]
    fn serialize_roundtrip() {
        let topic = new_topic::<SerTestType>();

        assert_eq!(topic.try_get(), None);
        assert_eq!(topic.try_get_as_bytes(), None);

        topic
            .set_from_bytes(br#"{"c": "test", "b": 1, "a": true}"#)
            .unwrap();

        assert_eq!(
            topic.try_get(),
            Some(SerTestType {
                a: true,
                b: 1,
                c: "test".to_string()
            })
        );

        let ser = topic.try_get_as_bytes().unwrap();
        let ser_str = std::str::from_utf8(ser.as_ref()).unwrap();

        assert_eq!(ser_str, r#"{"a":true,"b":1,"c":"test"}"#);
    }
}
