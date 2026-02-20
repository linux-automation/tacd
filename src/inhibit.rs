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
    path::PathBuf,
};

use crate::dut_power::OutputState;
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
const VAR_RUN_TACD_INHIBIT: &str = "demo_files/var/run/tacd/inhibit";

#[cfg(not(feature = "demo_mode"))]
const VAR_RUN_TACD_INHIBIT: &str = "/var/run/tacd/inhibit";

struct InhibitFile {
    name: &'static str,
}

impl InhibitFile {
    fn new(name: &'static str) -> Self {
        Self { name }
    }

    fn path(&self) -> PathBuf {
        let mut path: PathBuf = VAR_RUN_TACD_INHIBIT.into();
        path.push(self.name);
        path
    }

    fn inhibit(&self) -> std::io::Result<()> {
        create_dir_all(VAR_RUN_TACD_INHIBIT)?;
        File::create(self.path())?;

        Ok(())
    }

    fn release(&self) -> std::io::Result<()> {
        match remove_file(self.path()) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            res => res,
        }
    }
}

pub fn run(
    wtb: &mut WatchedTasksBuilder,
    dut_pwr: &crate::dut_power::DutPwrThread,
    setup_mode: &crate::setup_mode::SetupMode,
) -> anyhow::Result<()> {
    let (dut_pwr_state_events, _) = dut_pwr.state.clone().subscribe_unbounded();
    let dut_pwr_inhibit = InhibitFile::new("dut-pwr");

    wtb.spawn_task("inhibit-dut-pwr-service", async move {
        loop {
            match dut_pwr_state_events.recv().await? {
                OutputState::On => dut_pwr_inhibit.inhibit()?,
                _ => dut_pwr_inhibit.release()?,
            }
        }
    })?;

    let (setup_mode_events, _) = setup_mode.setup_mode.clone().subscribe_unbounded();
    let setup_mode_inhibit = InhibitFile::new("setup-mode");
    wtb.spawn_task("inhibit-setup-mode-service", async move {
        loop {
            match setup_mode_events.recv().await? {
                true => setup_mode_inhibit.inhibit()?,
                false => setup_mode_inhibit.release()?,
            }
        }
    })?;

    Ok(())
}
