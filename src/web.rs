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

use std::fs::write;
use std::net::TcpListener;

use tide::{Request, Response, Server};

#[cfg(any(test, feature = "stub_out_root"))]
mod sd {
    use std::io::Result;
    use std::net::TcpListener;

    pub const FALLBACK_PORT: &str = "[::]:8080";

    pub fn listen_fds(_: bool) -> Result<[(); 0]> {
        Ok([])
    }

    pub fn tcp_listener<E>(_: E) -> Result<TcpListener> {
        unimplemented!()
    }
}

#[cfg(not(any(test, feature = "stub_out_root")))]
mod sd {
    pub use systemd::daemon::*;

    pub const FALLBACK_PORT: &str = "[::]:80";
}

use sd::{listen_fds, tcp_listener, FALLBACK_PORT};

pub struct WebInterface {
    listeners: Vec<TcpListener>,
    pub server: Server<()>,
}

impl WebInterface {
    pub fn new() -> Self {
        let mut this = Self {
            listeners: Vec::new(),
            server: tide::new(),
        };

        // Use sockets provided by systemd (if any) to make socket activation
        // work
        if let Ok(fds) = listen_fds(true) {
            this.listeners
                .extend(fds.iter().filter_map(|fd| tcp_listener(fd).ok()));
        }

        // Open [::]:80 / [::]:8080 ourselves if systemd did not provide anything.
        // This, somewhat confusingly also listens on 0.0.0.0.
        if this.listeners.is_empty() {
            this.listeners.push(TcpListener::bind(FALLBACK_PORT).expect(
                "Could not bind web API to port, is there already another service running?",
            ));
        }

        this.expose_openapi_json();

        this
    }

    pub fn expose_openapi_json(&mut self) {
        self.server
            .at("/v1/openapi.json")
            .get(|req: Request<()>| async move {
                Ok(Response::builder(200)
                    .content_type("application/json")
                    .body(&include_bytes!(concat!(env!("OUT_DIR"), "/openapi.json"))[..])
                    .build())
            });
    }

    // Serve a file from disk for reading and writing
    pub fn expose_file_rw(&mut self, fs_path: &str, web_path: &str) {
        self.server.at(web_path).serve_file(fs_path).unwrap();

        let fs_path = fs_path.to_string();

        self.server.at(web_path).put(move |mut req: Request<()>| {
            let fs_path = fs_path.clone();

            async move {
                let content = req.body_bytes().await?;
                write(&fs_path, &content)?;

                Ok(Response::new(204))
            }
        });
    }

    pub async fn serve(self) -> Result<(), std::io::Error> {
        self.server.listen(self.listeners).await
    }
}
