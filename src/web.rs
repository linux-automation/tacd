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

use std::convert::AsRef;
use std::fs::write;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::Path;

use log::warn;
use tide::{Body, Response, Server};

#[cfg(feature = "demo_mode")]
mod consts {
    pub const WEBUI_DIR: &str = "web/build";
    pub const USER_DIR: &str = "srv/www";
    pub const FS_PREFIX: &str = "demo_files";
    pub const FALLBACK_PORT: &str = "[::]:8080";
}

#[cfg(not(feature = "demo_mode"))]
mod consts {
    pub const WEBUI_DIR: &str = "/usr/share/tacd/webui";
    pub const USER_DIR: &str = "/srv/www";
    pub const FS_PREFIX: &str = "";
    pub const FALLBACK_PORT: &str = "[::]:80";
}

use consts::{FALLBACK_PORT, FS_PREFIX, USER_DIR, WEBUI_DIR};

const OPENAPI_JSON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/openapi.json"));

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

        // Open [::]:80 / [::]:8080. This, somewhat confusingly also listens on
        // 0.0.0.0 and not only on IPv6.
        this.listeners.push(
            TcpListener::bind(FALLBACK_PORT).expect(
                "Could not bind web API to port, is there already another service running?",
            ),
        );

        this.expose_openapi_json();
        this.expose_dir(WEBUI_DIR, "/");
        this.expose_dir(USER_DIR, "/srv/");

        this
    }

    /// Serve a compiled-in openapi.json file
    fn expose_openapi_json(&mut self) {
        self.server.at("/v1/openapi.json").get(|_req| async move {
            let body = Body::from_bytes(OPENAPI_JSON.into());
            let response = Response::builder(200)
                .body(body)
                .content_type("application/json")
                .build();

            Ok(response)
        });
    }

    /// Serve a directory from disk for reading
    fn expose_dir(&mut self, fs_path: impl AsRef<Path>, web_path: &str) {
        if let Err(e) = self.server.at(web_path).serve_dir(&fs_path) {
            // Don't crash if the directory does not exist.
            // Just print a warning.
            match e.kind() {
                ErrorKind::NotFound => {
                    warn!(
                        "Can not serve {} at {}: Directory not found",
                        fs_path.as_ref().display(),
                        web_path
                    );
                }
                _ => Err(e).unwrap(),
            }
        }

        // Serve an index.html if the bare directory path is requested.
        // This only works for the top level. If we want to serve index.htmls
        // from sub-directories we would have to modify serve_dir().
        // Which is something we will likely want anyways as it does not
        // support compression, caching headers or directory listings.
        if web_path.ends_with('/') {
            let index_html = fs_path.as_ref().join("index.html");

            if let Err(e) = self.server.at(web_path).serve_file(&index_html) {
                // Don't crash if the directory does not exist. Just print a
                // warning.
                match e.kind() {
                    ErrorKind::NotFound => {
                        warn!(
                            "Can not serve {} at {}: File not found",
                            index_html.display(),
                            web_path
                        );
                    }
                    _ => Err(e).unwrap(),
                }
            }
        }
    }

    /// Serve a file from disk for reading and writing
    pub fn expose_file_rw(&mut self, fs_path: &str, web_path: &str) {
        let fs_path = FS_PREFIX.to_owned() + fs_path;

        self.server.at(web_path).serve_file(&fs_path).unwrap();

        self.server
            .at(web_path)
            .put(move |mut req: tide::Request<()>| {
                let fs_path = fs_path.clone();

                async move {
                    let content = req.body_bytes().await?;
                    write(&fs_path, content)?;

                    Ok(Response::new(204))
                }
            });
    }

    pub async fn serve(self) -> Result<(), std::io::Error> {
        self.server.listen(self.listeners).await
    }
}
