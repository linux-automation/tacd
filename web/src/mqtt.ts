import { Client, Message } from "paho-mqtt";
import { useEffect, useState } from "react";

export const session = new Client(
  `ws://${window.location.hostname}:${window.location.port}/v1/mqtt`,
  "webinterface-" + (Math.random() * 1000000).toFixed()
);

let subscriptions: {
  [topic: string]: Array<(message: Message | undefined) => void>;
} = {};

session.onConnectionLost = function (responseObject) {
  if (responseObject.errorCode !== 0) {
    console.log("onConnectionLost:" + responseObject.errorMessage);

    for (let topic in subscriptions) {
      for (let handler of subscriptions[topic]) {
        handler(undefined);
      }
    }
  }
};

session.onMessageArrived = function (message) {
  if (message.destinationName in subscriptions) {
    for (let handler of subscriptions[message.destinationName]) {
      handler(message);
    }
  }
};

session.connect({
  onSuccess: function () {
    for (let topic in subscriptions) {
      session.subscribe(topic);
    }
  },
  reconnect: true,
});

export function useMqttState<T>(topic: string, initial?: T) {
  const [shadow, setShadow] = useState<[boolean, T | undefined]>([
    false,
    initial,
  ]);

  useEffect(() => {
    function handleMessage(message: Message | undefined) {
      if (message !== undefined) {
        setShadow([true, JSON.parse(message.payloadString)]);
      } else {
        setShadow([false, undefined]);
      }
    }

    if (subscriptions[topic] === undefined) {
      if (session.isConnected()) {
        session.subscribe(topic);
      }

      subscriptions[topic] = [];
    }

    subscriptions[topic].push(handleMessage);

    return function cleanup() {
      const index = subscriptions[topic].indexOf(handleMessage, 0);

      if (index > -1) {
        subscriptions[topic].splice(index, 1);
      }

      if (subscriptions[topic].length === 0) {
        delete subscriptions[topic];

        if (session.isConnected()) {
          session.unsubscribe(topic);
        }
      }
    };
  }, [topic]);

  function setPayload(payload: T) {
    setShadow([false, payload]);
    session.send(topic, JSON.stringify(payload), 0, true);
  }

  const [settled, payload] = shadow;

  return [settled, payload, setPayload] as const;
}

export function useMqttSubscription<T>(topic: string, initial?: T) {
  // eslint-disable-next-line
  const [settled, payload, setPayload] = useMqttState<T>(topic, initial);
  return payload;
}
