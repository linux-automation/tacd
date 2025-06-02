// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2025 Pengutronix e.K.
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

use std::fmt::{self, Display, Formatter};
use std::fs::{create_dir_all, File};
use std::io::{Seek, Write};
use std::path::Path;

use anyhow::Result;
use futures::FutureExt;
use nix::errno::Errno;
use nix::mount::MsFlags;

use crate::dut_power::OutputState;
use crate::temperatures::Warning;
use crate::usb_hub::OverloadedPort;
use crate::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
mod setup {
    pub(super) const VAR_RUN_TACD: &str = "demo_files/var/run/tacd";
    pub(super) const ETC: &str = "demo_files/etc";

    /// umount stub for demo_mode that works without root permissions
    ///
    /// (by doing nothing).
    pub(super) fn umount(_target: &std::path::Path) -> nix::Result<()> {
        Err(nix::errno::Errno::EINVAL)
    }

    /// mount stub for demo_mode that works without root permissions
    ///
    /// (by doing nothing).
    pub(super) fn mount(
        _source: Option<&std::path::Path>,
        _target: &std::path::Path,
        _fstype: Option<&str>,
        _flags: nix::mount::MsFlags,
        _data: Option<&str>,
    ) -> nix::Result<()> {
        Ok(())
    }
}

#[cfg(not(feature = "demo_mode"))]
mod setup {
    pub(super) use nix::mount::{mount, umount};
    pub(super) const VAR_RUN_TACD: &str = "/var/run/tacd";
    pub(super) const ETC: &str = "/etc";
}

use setup::*;

struct Motd {
    dut_pwr_state: OutputState,
    iobus_fault: bool,
    rauc_should_reboot: bool,
    rauc_update_urls: Vec<String>,
    setup_mode_active: bool,
    temperature_warning: bool,
    usb_overload: Option<OverloadedPort>,
    handle: File,
}

const COLOR_RED: &str = "\x1b[31m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RESET: &str = "\x1b[0m";

impl Display for Motd {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Welcome to your TAC!")?;
        writeln!(f)?;

        if self.temperature_warning {
            writeln!(
                f,
                "- {COLOR_RED}WARNING{COLOR_RESET}: Your TAC is overheating, please provide proper airflow and let",
            )?;
            writeln!(f, "  it cool down.")?;
        }

        if self.setup_mode_active {
            writeln!(
                f,
                "- {COLOR_GREEN}GREAT!{COLOR_RESET} You have logged in successfully!",
            )?;
            writeln!(
                f,
                "  Now you should continue the setup process in the web interface"
            )?;
            writeln!(f, "  to leave the setup mode.")?;
        }

        if self.rauc_should_reboot {
            writeln!(
                f,
                "- {COLOR_YELLOW}INFO{COLOR_RESET}: A software update was installed. Please reboot to start using it.",
            )?;
        }

        if !self.rauc_update_urls.is_empty() {
            writeln!(
                f,
                "- {COLOR_YELLOW}INFO{COLOR_RESET}: A software update is available. To install it run:",
            )?;
            writeln!(f)?;

            for url in &self.rauc_update_urls {
                writeln!(f, "    rauc install \"{url}\"")?;
                writeln!(f)?;
            }
        }

        match self.dut_pwr_state {
            OutputState::On => {
                writeln!(
                    f,
                    "- {COLOR_GREEN}NOTE{COLOR_RESET}: The device under test is currently powered on.",
                )?;
            }
            OutputState::Off | OutputState::OffFloating | OutputState::Changing => {}
            OutputState::InvertedPolarity => {
                writeln!(
                        f,
                        "- {COLOR_RED}WARNING{COLOR_RESET}: The device under test was powered off due to inverted polarity.",
                    )?;
            }
            OutputState::OverCurrent => {
                writeln!(
                    f,
                    "- {COLOR_RED}WARNING{COLOR_RESET}: The device under test was powered off due to overcurrent.",
                )?;
            }
            OutputState::OverVoltage => {
                writeln!(
                    f,
                    "- {COLOR_RED}WARNING{COLOR_RESET}: The device under test was powered off due to overvoltage.",
                )?;
            }
            OutputState::RealtimeViolation => {
                writeln!(
                        f,
                        "- {COLOR_RED}WARNING{COLOR_RESET}: The device under test was powered because the TAC could not hold",
                    )?;

                writeln!(f, "  its realtime guarantees.",)?;
            }
        }

        if let Some(port) = &self.usb_overload {
            let port = match port {
                OverloadedPort::Total => " ",
                OverloadedPort::Port1 => " 1 ",
                OverloadedPort::Port2 => " 2 ",
                OverloadedPort::Port3 => " 3 ",
            };

            writeln!(
                f,
                "- {COLOR_RED}WARNING{COLOR_RESET}: The USB port{port}power supply is overloaded.",
            )?;
        }

        if self.iobus_fault {
            writeln!(
                f,
                "- {COLOR_RED}WARNING{COLOR_RESET}: The LXA IOBus power supply is overloaded.",
            )?;
        }

        Ok(())
    }
}

pub fn run(
    wtb: &mut WatchedTasksBuilder,
    dut_pwr: &crate::dut_power::DutPwrThread,
    iobus: &crate::iobus::IoBus,
    rauc: &crate::dbus::Rauc,
    setup_mode: &crate::setup_mode::SetupMode,
    temperatures: &crate::temperatures::Temperatures,
    usb_hub: &crate::usb_hub::UsbHub,
) -> Result<()> {
    let mut motd = Motd::new()?;

    // Write default MOTD once on startup
    motd.update()?;

    // Spawn a task that accepts motd updates and dumps them into the file in /var/run.
    let (state_events, _) = dut_pwr.state.clone().subscribe_unbounded();
    let (fault_events, _) = iobus.supply_fault.clone().subscribe_unbounded();
    let (should_reboot_events, _) = rauc.should_reboot.clone().subscribe_unbounded();
    let (channels_events, _) = rauc.channels.clone().subscribe_unbounded();
    let (setup_mode_events, _) = setup_mode.setup_mode.clone().subscribe_unbounded();
    let (temperature_events, _) = temperatures.warning.clone().subscribe_unbounded();
    let (usb_events, _) = usb_hub.overload.clone().subscribe_unbounded();

    wtb.spawn_task("motd-file-service", async move {
        loop {
            futures::select! {
                update = state_events.recv().fuse() => {
                    motd.dut_pwr_state = update?;
                },
                update = fault_events.recv().fuse() => {
                    motd.iobus_fault = update?;
                },
                update = should_reboot_events.recv().fuse() => {
                    motd.rauc_should_reboot = update?;
                },
                update = channels_events.recv().fuse() => {
                    motd.rauc_update_urls = update?
                        .into_iter()
                        .filter_map(|ch| {
                            ch.bundle
                            .as_ref()
                            .is_some_and(|b| b.newer_than_installed)
                            .then_some(ch.url)
                        })
                        .collect();
                },
                update = setup_mode_events.recv().fuse() => {
                    motd.setup_mode_active = update?;
                },
                update = temperature_events.recv().fuse() => {
                    motd.temperature_warning = match update? {
                        Warning::Okay => false,
                        Warning::SocHigh | Warning::SocCritical => true,
                    };
                },
                update = usb_events.recv().fuse() => {
                    motd.usb_overload = update?;
                },
            };

            motd.update()?;
        }
    })?;

    Ok(())
}

impl Motd {
    /// Create a motd in a tmpfs so we can write it without harming the eMMC
    fn new() -> Result<Self> {
        // Create /var/run/tacd (or an equivalent in demo mode).
        create_dir_all(VAR_RUN_TACD)?;

        // "/var/run/tacd/motd" or "demo_files/var/run/tacd/motd"
        // "/etc/motd" or "demo_files/etc/motd"
        let path_runtime_motd = Path::new(VAR_RUN_TACD).join("motd");
        let path_etc_motd = Path::new(ETC).join("motd");

        // Create the motd file in /var/run/tacd.
        let runtime_motd = File::create(&path_runtime_motd)?;

        // Try to unmount the bind mount at /etc/motd before trying to set up a new one.
        // Filter out the expected error for when /etc/motd is not a bind mount yet.
        umount(&path_etc_motd).or_else(|err| match err {
            Errno::EINVAL => Ok(()),
            _ => Err(err),
        })?;

        // Bind mount /var/run/tacd/motd to /etc/motd.
        // The benefit over writing to /etc/motd directly is that we do not
        // hammer the eMMC as much.
        // The benefit over a symlink is that the bind-mount does not persist
        // across rebots, leaving the /etc/motd point to a non-existing file.
        // The drawback of using a bind-mount is that it clutters up the output
        // of `mount` and that it requires special permissions that we do not
        // have in demo_mode.
        mount(
            Some(&path_runtime_motd),
            &path_etc_motd,
            None::<&str>,
            MsFlags::MS_BIND,
            None::<&str>,
        )?;

        Ok(Self {
            dut_pwr_state: OutputState::Off,
            iobus_fault: false,
            rauc_should_reboot: false,
            rauc_update_urls: Vec::new(),
            setup_mode_active: false,
            temperature_warning: false,
            usb_overload: None,
            handle: runtime_motd,
        })
    }

    fn update(&mut self) -> Result<()> {
        self.handle.rewind()?;
        self.handle.set_len(0)?;
        write!(&self.handle, "{self}")?;
        Ok(())
    }
}
