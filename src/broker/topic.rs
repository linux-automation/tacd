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

use async_std::channel::{unbounded, Receiver, Sender, TrySendError};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;

use serde::{de::DeserializeOwned, Serialize};

use unique_token::Unique;

use super::TopicName;

pub(super) struct RetainedValue<E> {
    native: Arc<E>,
    serialized: Option<Arc<[u8]>>,
}

impl<E: Serialize> RetainedValue<E> {
    pub(super) fn new(val: Arc<E>) -> Self {
        Self {
            native: val,
            serialized: None,
        }
    }

    fn native(&self) -> Arc<E> {
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

pub struct Topic<E> {
    pub(super) path: TopicName,
    pub(super) web_readable: bool,
    pub(super) web_writable: bool,
    pub(super) retained: Mutex<VecDeque<RetainedValue<E>>>,
    pub(super) retained_length: usize,
    pub(super) senders: Mutex<Vec<(Unique, Sender<Arc<E>>)>>,
    pub(super) senders_serialized: Mutex<Vec<(Unique, Sender<(TopicName, Arc<[u8]>)>)>>,
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
    pub async fn unsubscribe(self) {
        if let Some(topic) = self.topic.upgrade() {
            let mut senders = topic.senders.lock().await;

            if let Some(idx) = senders.iter().position(|(token, _)| *token == self.token) {
                senders.swap_remove(idx);
            }
        }
    }
}

#[async_trait]
pub trait AnySubscriptionHandle: Sync + Send {
    async fn unsubscribe(&self);
}

#[async_trait]
impl<E: Send + Sync> AnySubscriptionHandle for SubscriptionHandle<E, Serialized> {
    /// Unsubscribe a sender from the serialized topic values
    ///
    /// The sender may already have been unsubscribed if e.g. the receiving side
    /// was dropped and set() was called. This will not result in an error.
    async fn unsubscribe(&self) {
        if let Some(topic) = self.topic.upgrade() {
            let mut senders = topic.senders_serialized.lock().await;

            if let Some(idx) = senders.iter().position(|(token, _)| *token == self.token) {
                senders.swap_remove(idx);
            }
        }
    }
}

impl<E: Serialize + DeserializeOwned> Topic<E> {
    async fn set_arc_with_retain_lock(
        &self,
        msg: Arc<E>,
        retained: &mut VecDeque<RetainedValue<E>>,
    ) {
        // Do all locking up front and in a known order to prevent deadlocks
        let mut senders = self.senders.lock().await;
        let mut senders_serialized = self.senders_serialized.lock().await;

        let mut val = RetainedValue::new(msg);

        // Iterate through all native senders and try to enqueue the message.
        // In case of success keep the sender, if the (bounded) queue is full
        // close the queue (so that e.g. websockets are closed in the respective
        // task) and remove the sender from the list, if the queue is already
        // closed also remove it.
        senders.retain(|(_, s)| match s.try_send(val.native()) {
            Ok(_) => true,
            Err(TrySendError::Full(_)) => {
                s.close();
                false
            }
            Err(TrySendError::Closed(_)) => false,
        });

        // Iterate through all serialized senders and do as above
        senders_serialized.retain(|(_, s)| {
            match s.try_send((self.path.clone(), val.serialized())) {
                Ok(_) => true,
                Err(TrySendError::Full(_)) => {
                    s.close();
                    false
                }
                Err(TrySendError::Closed(_)) => false,
            }
        });

        retained.push_back(val);

        while retained.len() > self.retained_length {
            retained.pop_front();
        }
    }

    /// Set a new value for the topic and notify subscribers
    ///
    /// # Arguments
    ///
    /// * `msg` - Value to set the topic to (as Arc)
    pub async fn set_arc(&self, msg: Arc<E>) {
        let mut retained = self.retained.lock().await;

        self.set_arc_with_retain_lock(msg, &mut *retained).await
    }

    /// Set a new value for the topic and notify subscribers
    ///
    /// # Arguments
    ///
    /// * `msg` - Value to set the topic to
    pub async fn set(&self, msg: E) {
        self.set_arc(Arc::new(msg)).await
    }

    /// Get the current value
    ///
    /// Or nothing if none is set
    pub async fn try_get(&self) -> Option<Arc<E>> {
        self.retained.lock().await.back().map(|v| v.native())
    }

    // Get the value of this topic
    //
    // Waits for a value if none was set yet
    pub async fn get(self: &Arc<Self>) -> Arc<E> {
        let (mut rx, sub) = self.clone().subscribe_unbounded().await;
        let val = rx.next().await;
        sub.unsubscribe().await;

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
    pub async fn modify<F>(&self, cb: F)
    where
        F: FnOnce(Option<Arc<E>>) -> Option<Arc<E>>,
    {
        let mut retained = self.retained.lock().await;

        if let Some(new) = cb(retained.back().map(|v| v.native())) {
            self.set_arc_with_retain_lock(new, &mut *retained).await;
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
    pub async fn subscribe(
        self: Arc<Self>,
        sender: Sender<Arc<E>>,
    ) -> SubscriptionHandle<E, Native> {
        let token = Unique::new();

        // If there is a retained value try to enqueue it right away.
        // It that fails mimic what set_arc_with_retain_lock would do.
        let retained_send_res = {
            let retained = self.retained.lock().await;

            retained
                .back()
                .map(|val| sender.try_send(val.native()))
                .unwrap_or(Ok(()))
        };

        match retained_send_res {
            Ok(_) => {
                self.senders.lock().await.push((token, sender));
            }
            Err(TrySendError::Full(_)) => {
                sender.close();
            }
            Err(TrySendError::Closed(_)) => {}
        };

        SubscriptionHandle {
            topic: Arc::downgrade(&self),
            token: token,
            phantom: PhantomData,
        }
    }

    /// Create a new unbounded queue and subscribe it to the topic
    ///
    /// The returned SubscriptionHandle can be used to remove the sender again
    /// from the list of subscribers.
    /// If a retained value is present it will be enqueued immediately.
    pub async fn subscribe_unbounded(
        self: Arc<Self>,
    ) -> (Receiver<Arc<E>>, SubscriptionHandle<E, Native>) {
        let (tx, rx) = unbounded();
        (rx, self.subscribe(tx).await)
    }
}

#[async_trait]
pub trait AnyTopic: Sync + Send {
    fn path(&self) -> &TopicName;
    fn web_readable(&self) -> bool;
    fn web_writable(&self) -> bool;
    async fn set_from_bytes(&self, msg: &[u8]) -> serde_json::Result<()>;
    async fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
    ) -> Box<dyn AnySubscriptionHandle>;
    async fn try_get_as_bytes(&self) -> Option<Arc<[u8]>>;
}

#[async_trait]
impl<E: Serialize + DeserializeOwned + Send + Sync + 'static> AnyTopic for Topic<E> {
    fn path(&self) -> &TopicName {
        &self.path
    }

    fn web_readable(&self) -> bool {
        self.web_readable
    }

    fn web_writable(&self) -> bool {
        self.web_writable
    }

    /// De-Serialize a message and set the topic to the resulting value
    ///
    /// Returns an Err if deserialization failed.
    async fn set_from_bytes(&self, msg: &[u8]) -> serde_json::Result<()> {
        let msg = serde_json::from_slice(msg)?;
        self.set(msg).await;
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
    async fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
    ) -> Box<dyn AnySubscriptionHandle> {
        let token = Unique::new();
        let mut should_add = true;

        // If there are retained values try to enqueue them right away.
        // It that fails mimic what set_arc_with_retain_lock would do.
        for val in self.retained.lock().await.iter_mut() {
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

        if should_add {
            self.senders_serialized.lock().await.push((token, sender));
        }

        let handle = SubscriptionHandle {
            topic: Arc::downgrade(&self),
            token: token,
            phantom: PhantomData,
        };

        Box::new(handle)
    }

    /// Try to get the current serialized topic value
    ///
    /// Returns None if no value was set yet.
    async fn try_get_as_bytes(&self) -> Option<Arc<[u8]>> {
        self.retained
            .lock()
            .await
            .back_mut()
            .map(|v| v.serialized())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{AnyTopic, RetainedValue, Topic, TopicName};
    use async_std::channel::{unbounded, Receiver};
    use async_std::sync::{Arc, Mutex};
    use async_std::task::block_on;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct SerTestType {
        a: bool,
        b: u32,
        c: String,
    }

    fn new_topic<E>() -> Arc<Topic<E>> {
        let topic = Topic {
            path: TopicName::new("/").unwrap(),
            web_readable: true,
            web_writable: true,
            retained: Mutex::new(VecDeque::new()),
            retained_length: 1,
            senders: Mutex::new(Vec::new()),
            senders_serialized: Mutex::new(Vec::new()),
        };

        Arc::new(topic)
    }

    fn collect_native<E: Clone>(recv: Receiver<Arc<E>>) -> Vec<E> {
        std::iter::from_fn(|| recv.try_recv().ok().map(|v| v.as_ref().clone())).collect()
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
        block_on(async {
            let topic = new_topic::<u32>();

            let (native_1, native_handle_1) = topic.clone().subscribe_unbounded().await;
            let (native_2, native_handle_2) = topic.clone().subscribe_unbounded().await;
            let (native_3, native_handle_3) = topic.clone().subscribe_unbounded().await;

            let (ser_1, ser_handle_1) = {
                let (tx, rx) = unbounded();
                (rx, topic.clone().subscribe_as_bytes(tx).await)
            };

            let (ser_2, ser_handle_2) = {
                let (tx, rx) = unbounded();
                (rx, topic.clone().subscribe_as_bytes(tx).await)
            };

            let (ser_3, ser_handle_3) = {
                let (tx, rx) = unbounded();
                (rx, topic.clone().subscribe_as_bytes(tx).await)
            };

            assert_eq!(topic.senders.lock().await.len(), 3);
            assert_eq!(topic.senders_serialized.lock().await.len(), 3);

            topic.set(2).await;
            native_handle_2.unsubscribe().await;
            ser_handle_2.unsubscribe().await;

            assert_eq!(topic.senders.lock().await.len(), 2);
            assert_eq!(topic.senders_serialized.lock().await.len(), 2);

            topic.set(1).await;
            native_handle_1.unsubscribe().await;
            ser_handle_1.unsubscribe().await;

            assert_eq!(topic.senders.lock().await.len(), 1);
            assert_eq!(topic.senders_serialized.lock().await.len(), 1);

            topic.set(3).await;
            native_handle_3.unsubscribe().await;
            ser_handle_3.unsubscribe().await;

            assert_eq!(topic.senders.lock().await.len(), 0);
            assert_eq!(topic.senders_serialized.lock().await.len(), 0);

            topic.set(4).await;

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
        })
    }

    #[test]
    fn serialize_roundtrip() {
        block_on(async {
            let topic = new_topic::<SerTestType>();

            assert_eq!(topic.try_get().await, None);
            assert_eq!(topic.try_get_as_bytes().await, None);

            topic
                .set_from_bytes(br#"{"c": "test", "b": 1, "a": true}"#)
                .await
                .unwrap();

            assert_eq!(
                topic.try_get().await,
                Some(Arc::new(SerTestType {
                    a: true,
                    b: 1,
                    c: "test".to_string()
                }))
            );

            let ser = topic.try_get_as_bytes().await.unwrap();
            let ser_str = std::str::from_utf8(ser.as_ref()).unwrap();

            assert_eq!(ser_str, r#"{"a":true,"b":1,"c":"test"}"#);
        })
    }
}
