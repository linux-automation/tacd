// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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

use std::fs::{create_dir, rename, File};
use std::path::Path;

use anyhow::{bail, Result};
use async_std::channel::{unbounded, Receiver};
use async_std::prelude::*;
use async_std::sync::Arc;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{from_reader, to_writer_pretty, Map, Value};

use super::{AnyTopic, TopicName};

use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
const PERSISTENCE_PATH: &str = "demo_files/srv/tacd/state.json";

#[cfg(not(feature = "demo_mode"))]
const PERSISTENCE_PATH: &str = "/srv/tacd/state.json";

#[derive(Serialize, Deserialize)]
struct PersistenceFile {
    format_version: u64,
    persistent_topics: Map<String, Value>,
}

fn load(topics: &[Arc<dyn AnyTopic>]) -> Result<()> {
    let path = Path::new(PERSISTENCE_PATH);

    if !path.is_file() {
        info!(
            "State file at \"{}\" does not yet exist. Using defaults",
            PERSISTENCE_PATH
        );
        return Ok(());
    }

    let file: PersistenceFile = from_reader(File::open(path)?)?;

    if file.format_version != 1 {
        bail!("Unknown state file version: {}", file.format_version);
    }

    let mut content = file.persistent_topics;

    for topic in topics.iter().filter(|t| t.persistent()) {
        let path: &str = topic.path();

        if let Some(value) = content.remove(path) {
            topic.set_from_json_value(value)?;
        }
    }

    if !content.is_empty() {
        warn!("State file contained extra keys:");
        for topic_name in content.keys() {
            warn!(" - {topic_name}");
        }
    }

    Ok(())
}

fn save(topics: &Arc<Vec<Arc<dyn AnyTopic>>>) -> Result<()> {
    let persistent_topics = {
        let mut map = Map::new();

        for topic in topics.iter().filter(|t| t.persistent()) {
            let key = topic.path().to_string();
            let value = topic.try_get_json_value();

            if let Some(value) = value {
                if map.insert(key, value).is_some() {
                    let name: &str = topic.path();
                    error!("Duplicate persistent topic: \"{name}\"");
                    // continue anyways
                }
            }
        }

        map
    };

    let file_contents = PersistenceFile {
        format_version: 1,
        persistent_topics,
    };

    let path = Path::new(PERSISTENCE_PATH);
    let parent = path.parent().unwrap();

    let path_tmp = {
        let mut path_tmp = path.to_owned();
        assert!(path_tmp.set_extension("tmp"));
        path_tmp
    };

    if !parent.exists() {
        create_dir(parent)?;
    }

    {
        let fd = File::create(&path_tmp)?;
        to_writer_pretty(&fd, &file_contents)?;
        fd.sync_all()?;
    }

    rename(path_tmp, path)?;

    Ok(())
}

async fn save_on_change(
    topics: Arc<Vec<Arc<dyn AnyTopic>>>,
    mut change_ev: Receiver<(TopicName, Arc<[u8]>)>,
) -> Result<()> {
    while let Some((topic_name, _)) = change_ev.next().await {
        let topic_name = String::from_utf8_lossy(topic_name.as_bytes());

        info!(
            "Persistent topic \"{}\" has changed. Saving to disk",
            topic_name
        );

        save(&topics)?;
    }

    Ok(())
}

pub fn register(wtb: &mut WatchedTasksBuilder, topics: Arc<Vec<Arc<dyn AnyTopic>>>) -> Result<()> {
    load(&topics).unwrap();

    let (tx, rx) = unbounded();

    for topic in topics.iter().filter(|t| t.persistent()).cloned() {
        topic.subscribe_as_bytes(tx.clone(), false);
    }

    wtb.spawn_task("persistence-save", save_on_change(topics, rx))
}
