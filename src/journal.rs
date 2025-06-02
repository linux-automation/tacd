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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use async_std::channel::bounded;
use async_std::io::BufReader;
use async_std::prelude::*;
use async_std::task::{block_on, spawn_blocking};

use serde::Deserialize;
use serde_json::to_string;
use tide::http::Body;
use tide::{Request, Response, Server};

#[cfg(any(test, feature = "demo_mode"))]
mod sd {
    use std::collections::btree_map::BTreeMap;
    use std::io::Error;
    pub(super) use std::io::Result;
    use std::thread::sleep;
    use std::time::{Duration, SystemTime};

    pub(super) type JournalRecord = BTreeMap<String, String>;
    pub(super) struct Journal;
    pub(super) struct OpenOptions;

    impl OpenOptions {
        pub fn default() -> Self {
            Self
        }

        pub fn system(self, _: bool) -> Self {
            self
        }

        pub fn local_only(self, _: bool) -> Self {
            self
        }

        pub fn open(self) -> Result<Journal> {
            Ok(Journal)
        }
    }

    impl Journal {
        pub fn seek_tail(&mut self) -> Result<()> {
            Ok(())
        }

        pub fn previous_entry(&mut self) -> Result<Option<JournalRecord>> {
            Ok(None)
        }

        pub fn watch_all_elements<F>(&mut self, mut f: F) -> Result<()>
        where
            F: FnMut(JournalRecord) -> Result<()>,
        {
            for _i in 0..10 {
                let ts = SystemTime::UNIX_EPOCH.elapsed().unwrap().as_micros();

                let mut rec = JournalRecord::new();
                rec.insert("_SOURCE_REALTIME_TIMESTAMP".to_string(), format!("{ts}"));
                rec.insert("UNIT".to_string(), "tacd.service".to_string());
                rec.insert("MESSAGE".to_string(), "Says HI!".to_string());

                f(rec)?;

                sleep(Duration::from_secs(5));
            }

            Err(Error::other("Simulation ended"))
        }
    }
}

#[cfg(not(any(test, feature = "demo_mode")))]
mod sd {
    pub(super) use systemd::journal::*;
    pub(super) use systemd::*;
}

use sd::{Journal, JournalRecord, OpenOptions, Result};

#[derive(Deserialize)]
struct QueryParams {
    history_len: Option<u64>,
    unit: Option<String>,
}

struct UnitFilter {
    unit: Option<String>,
}

impl UnitFilter {
    pub fn new(unit: Option<String>) -> Self {
        Self { unit }
    }

    pub fn filter(&self, record: JournalRecord) -> Option<JournalRecord> {
        let unit = record.get("UNIT").or(record.get("_SYSTEMD_UNIT"));

        // Send the entry if the unit matches or no filter was set
        let should_send = match (unit, self.unit.as_ref()) {
            (_, None) => true,
            (Some(u), Some(f)) => u == f,
            (None, Some(_)) => false,
        };

        if should_send {
            Some(record)
        } else {
            None
        }
    }
}

fn open_journal(mut history_len: u64, filter: &UnitFilter) -> Result<Journal> {
    let mut journal = OpenOptions::default()
        .system(true)
        .local_only(true)
        .open()?;

    journal.seek_tail()?;

    let mut element_limit = 2048;

    // Try to go back far enough to have history_len entries matching the
    // specified filter in in the backlog. But limit the effort to a maximum
    // number of elements to look at.
    while (history_len > 0) && (element_limit > 0) {
        if let Some(entry) = journal.previous_entry()? {
            if filter.filter(entry).is_some() {
                history_len -= 1;
            }

            element_limit -= 1
        } else {
            element_limit = 0;
        }
    }

    Ok(journal)
}

pub fn serve(server: &mut Server<()>) {
    server
        .at("/v1/tac/journal")
        .get(|req: Request<()>| async move {
            let (response_tx, mut response_rx) = bounded::<Response>(1);

            // The Journal is not Send, so it has to be set up in the thread
            // that uses it.
            // It would however be nice to return a HTTP error code if the
            // setup process fails early on.
            // This is why we have this channel contraption, which sends a single
            // response back to be sent to the client.
            spawn_blocking(move || {
                let (sender, mut journal, filter) = {
                    let (unit, history_len) = match req.query() {
                        Ok(QueryParams { history_len, unit }) => (unit, history_len),
                        Err(e) => {
                            let resp = Response::builder(500)
                                .body(format!("Failed to parse query parameters: {e}"))
                                .build();
                            let _ = response_tx.try_send(resp);
                            return;
                        }
                    };

                    let filter = UnitFilter::new(unit);

                    let journal = match open_journal(history_len.unwrap_or(10), &filter) {
                        Ok(j) => j,
                        Err(e) => {
                            let resp = Response::builder(500)
                                .body(format!("Failed to open journal file(s): {e}"))
                                .build();
                            let _ = response_tx.try_send(resp);
                            return;
                        }
                    };

                    // The journal was opened successfully, we can send a successful
                    // response to the client.
                    let (sender, encoder) = async_sse::encode();

                    let resp = Response::builder(200)
                        .body(Body::from_reader(BufReader::new(encoder), None))
                        .header("Cache-Control", "no-cache")
                        .content_type(tide::http::mime::SSE)
                        .build();

                    if response_tx.try_send(resp).is_err() {
                        // The Future handling the get request was canceled, the
                        // response Receiver dropped and thus the channel closed.
                        return;
                    }

                    (sender, journal, filter)
                };

                let sender_watch = sender.clone();
                let res = journal.watch_all_elements(move |element| {
                    if let Some(elem) = filter.filter(element) {
                        let json = to_string(&elem)?;
                        block_on(sender_watch.send("entry", &json, None))?;
                    }

                    Ok(())
                });

                // An error occurred once we have already set up the SSE session
                // (e.g. a success was already signaled via HTTP response code).
                // Use an extra "error" SSE topic to somehow inform the client
                // anyways.
                if let Err(e) = res {
                    let _ = block_on(sender.send("error", &e.to_string(), None));
                }
            });

            let resp = response_rx.next().await.unwrap_or(
                Response::builder(500)
                    .body("Journal reader stopped unexpectedly")
                    .build(),
            );

            Ok(resp)
        });
}
