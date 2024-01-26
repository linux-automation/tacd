use std::fs::{create_dir_all, File};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use async_std::stream::StreamExt;
use log::warn;
use nix::errno::Errno;
use nix::mount::MsFlags;
use serde::{Deserialize, Serialize};

use crate::broker::Topic;
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

#[derive(Clone, Serialize, Deserialize)]
struct MotdContent {
    dut_pwr_state: OutputState,
    iobus_fault: bool,
    rauc_should_reboot: bool,
    rauc_update_urls: Vec<String>,
    setup_mode_active: bool,
    temperature_warning: bool,
    usb_overload: Option<OverloadedPort>,
}

pub struct Motd {
    path_etc_motd: PathBuf,
}

const COLOR_RED: &str = "\x1b[31m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RESET: &str = "\x1b[0m";

impl MotdContent {
    fn write(&self, dst: &mut File) -> std::io::Result<()> {
        writeln!(dst, "Welcome to you TAC!")?;
        writeln!(dst)?;

        if self.temperature_warning {
            writeln!(
                dst,
                "- {}WARNING{}: Your TAC is overheating, please provide proper airflow and let",
                COLOR_RED, COLOR_RESET
            )?;
            writeln!(dst, "  it cool down.")?;
        }

        if self.setup_mode_active {
            writeln!(
                dst,
                "- {}GREAT!{} You have logged in successfully!",
                COLOR_GREEN, COLOR_RESET
            )?;
            writeln!(
                dst,
                "  Now you should continue the setup process in the web interface"
            )?;
            writeln!(dst, "  to leave the setup mode.")?;
        }

        if self.rauc_should_reboot {
            writeln!(
                dst,
                "- {}INFO{}: A software update was installed. Please reboot to start using it.",
                COLOR_YELLOW, COLOR_RESET
            )?;
        }

        if !self.rauc_update_urls.is_empty() {
            writeln!(
                dst,
                "- {}INFO{}: A software update is available. To install it run:",
                COLOR_YELLOW, COLOR_RESET
            )?;
            writeln!(dst)?;

            for url in &self.rauc_update_urls {
                writeln!(dst, "    rauc install \"{url}\"")?;
                writeln!(dst)?;
            }
        }

        match self.dut_pwr_state {
            OutputState::On => {
                writeln!(
                    dst,
                    "- {}NOTE{}: The device under test is currently powered on.",
                    COLOR_GREEN, COLOR_RESET
                )?;
            }
            OutputState::Off | OutputState::OffFloating | OutputState::Changing => {}
            OutputState::InvertedPolarity => {
                writeln!(
                    dst,
                    "- {}WARNING{}: The device under test was powered off due to inverted polarity.",
                    COLOR_RED,
                    COLOR_RESET
                )?;
            }
            OutputState::OverCurrent => {
                writeln!(
                    dst,
                    "- {}WARNING{}: The device under test was powered off due to overcurrent.",
                    COLOR_RED, COLOR_RESET
                )?;
            }
            OutputState::OverVoltage => {
                writeln!(
                    dst,
                    "- {}WARNING{}: The device under test was powered off due to overvoltage.",
                    COLOR_RED, COLOR_RESET
                )?;
            }
            OutputState::RealtimeViolation => {
                writeln!(
                    dst,
                    "- {}WARNING{}: The device under test was powered because the TAC could not hold",
                    COLOR_RED,
                    COLOR_RESET
                )?;

                writeln!(dst, "  its realtime guarantees.",)?;
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
                dst,
                "- {}WARNING{}: The USB port{}power supply is overloaded.",
                COLOR_RED, COLOR_RESET, port
            )?;
        }

        if self.iobus_fault {
            writeln!(
                dst,
                "- {}WARNING{}: The LXA IOBus power supply is overloaded.",
                COLOR_RED, COLOR_RESET,
            )?;
        }

        Ok(())
    }
}

impl Default for MotdContent {
    fn default() -> Self {
        Self {
            dut_pwr_state: OutputState::Off,
            iobus_fault: false,
            rauc_should_reboot: false,
            rauc_update_urls: Vec::new(),
            setup_mode_active: false,
            temperature_warning: false,
            usb_overload: None,
        }
    }
}

impl Motd {
    pub fn new(
        wtb: &mut WatchedTasksBuilder,
        dut_pwr: &crate::dut_power::DutPwrThread,
        iobus: &crate::iobus::IoBus,
        rauc: &crate::dbus::Rauc,
        setup_mode: &crate::setup_mode::SetupMode,
        temperatures: &crate::temperatures::Temperatures,
        usb_hub: &crate::usb_hub::UsbHub,
    ) -> Result<Self> {
        let (mut motd, path_etc_motd) = Self::setup_run_tacd_motd()?;
        let content = Topic::<MotdContent>::anonymous(None);

        // Spawn a task that accepts motd updates from an anonymous topic
        // and dumps them into the file in /var/run.
        let (mut content_events, _) = content.clone().subscribe_unbounded();
        wtb.spawn_task("motd-write-file", async move {
            while let Some(event) = content_events.next().await {
                motd.rewind()?;
                motd.set_len(0)?;
                event.write(&mut motd)?;
            }

            Ok(())
        })?;

        // Spawn a lot of tasks that listen to other topics and update their
        // respective field in the motd topic.

        let (mut state_events, _) = dut_pwr.state.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-dut-pwr-state", async move {
            while let Some(event) = state_events.next().await {
                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.dut_pwr_state = event;

                    Some(val)
                })
            }

            Ok(())
        })?;

        let (mut fault_events, _) = iobus.supply_fault.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-iobus-fault", async move {
            while let Some(event) = fault_events.next().await {
                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.iobus_fault = event;

                    Some(val)
                })
            }

            Ok(())
        })?;

        let (mut should_reboot_events, _) = rauc.should_reboot.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-rauc-should-reboot", async move {
            while let Some(event) = should_reboot_events.next().await {
                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.rauc_should_reboot = event;

                    Some(val)
                })
            }

            Ok(())
        })?;

        let (mut channels_events, _) = rauc.channels.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-rauc-update-urls", async move {
            while let Some(channels) = channels_events.next().await {
                let update_urls = channels
                    .into_iter()
                    .filter_map(|ch| {
                        ch.bundle
                            .as_ref()
                            .map_or(false, |b| b.newer_than_installed)
                            .then_some(ch.url)
                    })
                    .collect();

                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.rauc_update_urls = update_urls;

                    Some(val)
                })
            }

            Ok(())
        })?;

        let (mut setup_mode_events, _) = setup_mode.setup_mode.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-setup-mode", async move {
            while let Some(event) = setup_mode_events.next().await {
                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.setup_mode_active = event;

                    Some(val)
                })
            }

            Ok(())
        })?;

        let (mut temperature_events, _) = temperatures.warning.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-temperature-warnings", async move {
            while let Some(event) = temperature_events.next().await {
                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.temperature_warning = match event {
                        Warning::Okay => false,
                        Warning::SocHigh | Warning::SocCritical => true,
                    };

                    Some(val)
                })
            }

            Ok(())
        })?;

        let (mut usb_events, _) = usb_hub.overload.clone().subscribe_unbounded();
        let content_clone = content.clone();
        wtb.spawn_task("motd-update-usb-overload", async move {
            while let Some(event) = usb_events.next().await {
                content_clone.modify(|prev| {
                    let mut val = prev.unwrap_or_default();

                    val.usb_overload = event;

                    Some(val)
                })
            }

            Ok(())
        })?;

        Ok(Motd { path_etc_motd })
    }

    /// Create a motd in a tmpfs so we can write it without harming the eMMC
    fn setup_run_tacd_motd() -> Result<(File, PathBuf)> {
        let var_run_tacd = Path::new(VAR_RUN_TACD);
        let etc = Path::new(ETC);

        // Create /var/run/tacd (or an equivalent in demo mode).
        create_dir_all(VAR_RUN_TACD)?;

        // "/var/run/tacd/motd" or "demo_files/var/run/tacd/motd"
        // "/etc/motd" or "demo_files/etc/motd"
        let path_runtime_motd = var_run_tacd.join("motd");
        let path_etc_motd = etc.join("motd");

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

        Ok((runtime_motd, path_etc_motd))
    }

    pub fn remove(self) {
        // Remove the bind mount at /etc/motd before exiting
        if let Err(e) = umount(&self.path_etc_motd) {
            warn!("Failed to remove /etc/motd bind mount: {}", e.desc());
            // There is not much more we can do about it
        }
    }
}
