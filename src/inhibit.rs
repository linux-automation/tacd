// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2026 Pengutronix e.K.
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

use std::{
    fs::{File, create_dir_all, remove_file},
    io::ErrorKind,
    path::Path,
};

use crate::watched_tasks::WatchedTasksBuilder;

pub struct InhibitFile {
    pub path: &'static str,
}

pub struct InhibitFiles {
    pub setup_mode: InhibitFile,
}

#[cfg(feature = "demo_mode")]
const FILES: InhibitFiles = InhibitFiles {
    setup_mode: InhibitFile {
        path: "demo_files/run/tacd/inhibit/setup-mode",
    },
};

#[cfg(not(feature = "demo_mode"))]
const FILES: InhibitFiles = InhibitFiles {
    setup_mode: InhibitFile {
        path: "/run/tacd/inhibit/setup-mode",
    },
};

impl InhibitFile {
    fn inhibit(&self) -> std::io::Result<()> {
        let path = Path::new(self.path);
        create_dir_all(path.parent().unwrap())?;
        File::create(path)?;

        Ok(())
    }

    fn release(&self) -> std::io::Result<()> {
        match remove_file(self.path) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            res => res,
        }
    }
}

impl InhibitFiles {
    pub fn get() -> &'static Self {
        &FILES
    }

    pub fn keep_updated(
        &'static self,
        wtb: &mut WatchedTasksBuilder,
        setup_mode: &crate::setup_mode::SetupMode,
    ) -> anyhow::Result<()> {
        let (setup_mode_events, _) = setup_mode.setup_mode.clone().subscribe_unbounded();
        wtb.spawn_task("inhibit-setup-mode-service", async move {
            loop {
                match setup_mode_events.recv().await? {
                    true => self.setup_mode.inhibit()?,
                    false => self.setup_mode.release()?,
                }
            }
        })?;

        Ok(())
    }
}
