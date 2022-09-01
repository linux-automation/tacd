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
    pub(super) retained: Mutex<Option<RetainedValue<E>>>,
    pub(super) senders: Mutex<Vec<(Unique, Sender<Arc<E>>)>>,
    pub(super) senders_serialized: Mutex<Vec<(Unique, Sender<(TopicName, Arc<[u8]>)>)>>,
}

pub struct Native;
pub struct Encoded;

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

impl<E> SubscriptionHandle<E, Encoded> {
    pub async fn unsubscribe(&self) {
        if let Some(topic) = self.topic.upgrade() {
            let mut senders = topic.senders_serialized.lock().await;

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
impl<E: Send + Sync> AnySubscriptionHandle for SubscriptionHandle<E, Encoded> {
    /// Unsubscribe a sender from the serialized topic values
    ///
    /// The sender may already have been unsubscribed if e.g. the receiving side
    /// was dropped and set() was called. This will not result in an error.
    async fn unsubscribe(&self) {
        Self::unsubscribe(self).await
    }
}

impl<E: Serialize + DeserializeOwned> Topic<E> {
    async fn set_arc_with_retain_lock(&self, msg: Arc<E>, retained: &mut Option<RetainedValue<E>>) {
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

        *retained = Some(val);
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

    pub async fn get(&self) -> Option<Arc<E>> {
        self.retained.lock().await.as_ref().map(|r| r.native())
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

        if let Some(new) = cb(retained.as_ref().map(|v| v.native())) {
            self.set_arc_with_retain_lock(new, &mut *retained).await;
        }
    }

    /// Add the provided sender to the list of subscribers
    ///
    /// The returned SubscriptionHandle can be used to remove the sender again
    /// from the list of subscribers. The subscriber will also be removed
    /// implicitly on the first `set` call after the recieving end of the queue
    /// was dropped.
    ///
    /// # Arguments
    ///
    /// * `sender` - The sender side of the queue to subscribe
    pub async fn subscribe(
        self: Arc<Self>,
        sender: Sender<Arc<E>>,
    ) -> SubscriptionHandle<E, Native> {
        let token = Unique::new();
        self.senders.lock().await.push((token, sender));

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
    pub async fn subscribe_unbounded(
        self: Arc<Self>,
    ) -> (Receiver<Arc<E>>, SubscriptionHandle<E, Native>) {
        let (tx, rx) = unbounded();
        (rx, self.subscribe(tx).await)
    }

    async fn set_from_bytes<'a>(&self, msg: &[u8]) -> Result<(), ()> {
        match serde_json::from_slice(msg) {
            Ok(m) => Ok(self.set(m).await),
            Err(_) => Err(()),
        }
    }

    async fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
    ) -> SubscriptionHandle<E, Encoded> {
        let token = Unique::new();
        self.senders_serialized.lock().await.push((token, sender));

        SubscriptionHandle {
            topic: Arc::downgrade(&self),
            token: token,
            phantom: PhantomData,
        }
    }

    async fn get_as_bytes(&self) -> Option<Arc<[u8]>> {
        let mut retained_serialized = self.retained_serialized.lock().await;

        if retained_serialized.is_none() {
            if let Some(native) = self.get().await {
                let encoded = serde_json::to_vec(&native).unwrap();
                *retained_serialized = Some(Arc::from(encoded.into_boxed_slice()));
            }
        }

        retained_serialized.as_ref().cloned()
    }
}

#[async_trait]
pub trait AnyTopic: Sync + Send {
    fn path(&self) -> &TopicName;
    fn web_readable(&self) -> bool;
    fn web_writable(&self) -> bool;
    async fn set_from_bytes(&self, msg: &[u8]) -> Result<(), ()>;
    async fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
    ) -> Box<dyn AnySubscriptionHandle>;
    async fn get_as_bytes(&self) -> Option<Arc<[u8]>>;
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

    async fn set_from_bytes(&self, msg: &[u8]) -> Result<(), ()> {
        self.set_from_bytes(msg).await
    }

    /// Add a queue to the list of subscribers for serialized values
    ///
    /// The Returned AnySubscriptionHandle can be used to remove the queue
    /// again from the list of subscribers.
    ///
    /// # Arguments:
    ///
    /// * `sender` - The sender side of the queue to add
    async fn subscribe_as_bytes(
        self: Arc<Self>,
        sender: Sender<(TopicName, Arc<[u8]>)>,
    ) -> Box<dyn AnySubscriptionHandle> {
        Box::new(self.subscribe_as_bytes(sender).await)
    }

    async fn get_as_bytes(&self) -> Option<Arc<[u8]>> {
        self.get_as_bytes().await
    }
}
