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

use std::fs::{create_dir_all, read, write};
use std::io::ErrorKind;
use std::path::Path;

use anyhow::Result;
use async_std::prelude::*;
use async_std::sync::Arc;
use tide::{http::mime, Request, Response, Server};

use crate::broker::{BrokerBuilder, Topic};
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
const AUTHORIZED_KEYS_PATH: &str = "demo_files/home/root/ssh/authorized_keys";

#[cfg(not(feature = "demo_mode"))]
const AUTHORIZED_KEYS_PATH: &str = "/home/root/.ssh/authorized_keys";

pub struct SetupMode {
    pub setup_mode: Arc<Topic<bool>>,
    pub show_help: Arc<Topic<bool>>,
}

impl SetupMode {
    fn expose_file_conditionally(
        &self,
        server: &mut Server<()>,
        fs_path: &'static str,
        web_path: &str,
    ) {
        let setup_mode_task = self.setup_mode.clone();
        server.at(web_path).put(move |mut req: Request<()>| {
            let setup_mode = setup_mode_task.clone();

            async move {
                let res = if setup_mode.get().await {
                    let fs_path = Path::new(fs_path);
                    let parent = fs_path.parent().unwrap();

                    if !parent.exists() {
                        create_dir_all(parent)?;
                    }

                    let content = req.body_bytes().await?;
                    write(fs_path, content)?;

                    Response::new(204)
                } else {
                    Response::builder(403)
                        .body("This file may only be written in setup mode")
                        .content_type(mime::PLAIN)
                        .build()
                };

                Ok(res)
            }
        });

        let setup_mode_task = self.setup_mode.clone();
        server.at(web_path).get(move |_| {
            let setup_mode = setup_mode_task.clone();

            async move {
                let res = if setup_mode.get().await {
                    match read(fs_path) {
                        Ok(content) => Response::builder(200)
                            .body(content)
                            .content_type(mime::PLAIN)
                            .build(),
                        Err(e) => {
                            let status = match e.kind() {
                                ErrorKind::NotFound => 404,
                                _ => 500,
                            };
                            Response::builder(status)
                                .body("Failed to read file")
                                .content_type(mime::PLAIN)
                                .build()
                        }
                    }
                } else {
                    Response::builder(403)
                        .body("This file may only be read in setup mode")
                        .content_type(mime::PLAIN)
                        .build()
                };

                Ok(res)
            }
        });
    }

    fn handle_leave_requests(
        &self,
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
    ) -> Result<()> {
        // Use the "register a read-only and a write-only topic with the same name
        // to perform validation" trick that is also used with the DUT power endpoint.
        // We must make sure that a client from the web can only ever trigger _leaving_
        // the setup mode, as they would otherwise be able to take over the TAC.
        let (mut leave_requests, _) = bb
            .topic_wo::<bool>("/v1/tac/setup_mode", None)
            .subscribe_unbounded();
        let setup_mode = self.setup_mode.clone();

        wtb.spawn_task("setup-mode-leave-request", async move {
            while let Some(lr) = leave_requests.next().await {
                if !lr {
                    // Only ever set the setup mode to false in here
                    setup_mode.set(false)
                }
            }

            Ok(())
        })
    }

    pub fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        server: &mut Server<()>,
    ) -> Result<Self> {
        let this = Self {
            setup_mode: bb.topic("/v1/tac/setup_mode", true, false, true, Some(true), 1),
            show_help: bb.topic(
                "/v1/tac/display/show_help",
                true,
                false,
                true,
                Some(true),
                1,
            ),
        };

        this.handle_leave_requests(bb, wtb)?;
        this.expose_file_conditionally(server, AUTHORIZED_KEYS_PATH, "/v1/tac/ssh/authorized_keys");

        Ok(this)
    }
}
