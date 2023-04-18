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

use tide::{Request, Response};

use super::AnyTopic;

async fn get_handler(topic: Arc<dyn AnyTopic>, mut _req: Request<()>) -> tide::Result {
    topic
        .try_get_as_bytes()
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
