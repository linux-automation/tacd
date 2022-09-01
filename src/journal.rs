use async_std::channel::bounded;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{block_on, spawn_blocking};

use serde::Deserialize;
use serde_json::to_string;
use tide::sse::endpoint;
use tide::{Error, Server};

#[cfg(any(test, feature = "stub_out_root"))]
mod sd {
    use std::collections::btree_map::BTreeMap;
    pub use std::io::Result;
    use std::thread::sleep;
    use std::time::{Duration, SystemTime};

    pub type JournalRecord = BTreeMap<String, String>;
    pub struct Journal;
    pub struct OpenOptions;

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

        pub fn previous_skip(&mut self, _: u64) -> Result<()> {
            Ok(())
        }

        pub fn watch_all_elements<F>(&mut self, mut f: F) -> Result<()>
        where
            F: FnMut(JournalRecord) -> Result<()>,
        {
            loop {
                let ts = SystemTime::UNIX_EPOCH.elapsed().unwrap().as_micros();

                let mut rec = JournalRecord::new();
                rec.insert("_SOURCE_REALTIME_TIMESTAMP".to_string(), format!("{ts}"));
                rec.insert("UNIT".to_string(), "tacd.service".to_string());
                rec.insert("MESSAGE".to_string(), "Says HI!".to_string());

                f(rec)?;

                sleep(Duration::from_secs(5));
            }
        }
    }
}

#[cfg(not(any(test, feature = "stub_out_root")))]
mod sd {
    pub use systemd::journal::*;
    pub use systemd::*;
}

use sd::{Journal, OpenOptions, Result};

#[derive(Deserialize)]
struct QueryParams {
    history_len: Option<u64>,
    unit: Option<String>,
}

fn open_journal(history_len: u64) -> Result<Journal> {
    let mut journal = OpenOptions::default()
        .system(true)
        .local_only(true)
        .open()?;

    journal.seek_tail()?;
    journal.previous_skip(history_len)?;
    Ok(journal)
}

pub fn serve(server: &mut Server<()>) {
    server
        .at("/v1/tac/journal")
        .get(endpoint(|req, sender| async move {
            let query: QueryParams = req.query()?;

            // The Journal is not Send, so it has to be set up in the thread
            // that uses it.
            // It would however be nice to return a HTTP error code if the
            // setup process fails early on.
            // This is why we have this channel contraption, which sends a
            // single error or success code when the journal is opened to
            // inform the client.
            // TODO: check if the sse endpoint implementation actually allows
            // this
            let (early_tx, mut early_rx) = bounded::<Result<()>>(1);

            spawn_blocking(move || {
                let sender = Arc::new(sender);

                let mut journal = match open_journal(query.history_len.unwrap_or(10)) {
                    Ok(j) => {
                        let _ = early_tx.try_send(Ok(()));
                        j
                    }
                    Err(e) => {
                        let _ = early_tx.try_send(Err(e));
                        return;
                    }
                };

                let sender_watch = sender.clone();
                let res = journal.watch_all_elements(move |element| {
                    let unit = element.get("UNIT").or(element.get("_SYSTEMD_UNIT"));

                    // Send the entry if the unit matches or no filter was set
                    let should_send = match (unit, query.unit.as_ref()) {
                        (_, None) => true,
                        (Some(u), Some(f)) => u == f,
                        (None, Some(_)) => false,
                    };

                    if should_send {
                        let json = to_string(&element)?;
                        block_on(sender_watch.send("entry", &json, None))?;
                    }

                    Ok(())
                });

                // An error occured once we have already set up the SSE session
                // signal it on an extra "error" topic.
                if let Err(e) = res {
                    let _ = block_on(sender.send("error", &e.to_string(), None));
                }
            });

            if let Some(res) = early_rx.next().await {
                res.map_err(|err| Error::from_str(500, err))
            } else {
                Err(Error::from_str(500, "Journal reader stopped unexpectely"))
            }
            .into()
        }));
}
