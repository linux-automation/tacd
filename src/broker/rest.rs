use async_std::sync::Arc;

use tide::{Request, Response};

use super::AnyTopic;

async fn get_handler(topic: Arc<dyn AnyTopic>, mut _req: Request<()>) -> tide::Result {
    topic
        .try_get_as_bytes()
        .await
        .ok_or(tide::Error::from_str(
            404,
            "Don't have a retained message yet",
        ))
        .map(|r| {
            tide::Response::builder(200)
                .body(r.to_vec())
                .content_type("application/json")
                .build()
        })
}

async fn put_handler(topic: Arc<dyn AnyTopic>, mut req: Request<()>) -> tide::Result {
    topic
        .set_from_bytes(&req.body_bytes().await?)
        .await
        .map(|_| Response::new(204))
        .map_err(|_| tide::Error::from_str(400, "Malformed payload"))
}

pub(super) fn register(server: &mut tide::Server<()>, topics: Arc<Vec<Arc<dyn AnyTopic>>>) {
    for topic in topics.iter() {
        let mut route = server.at(topic.path());

        if topic.web_readable() {
            let topic_clone = topic.clone();
            route.get(move |req| get_handler(topic_clone.clone(), req));
        }

        if topic.web_writable() {
            let topic_clone = topic.clone();
            route.put(move |req| put_handler(topic_clone.clone(), req));

            let topic_clone = topic.clone();
            route.post(move |req| put_handler(topic_clone.clone(), req));
        }
    }
}
