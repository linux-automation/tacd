use std::fs::{create_dir_all, rename, File};
use std::io::{Seek, Write};
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process;
use std::time::Duration;

use anyhow::Result;
use async_std::stream::StreamExt;
use async_std::task::sleep;
use serde::{Deserialize, Serialize};

use crate::broker::Topic;
use crate::dut_power::OutputState;
use crate::temperatures::Warning;
use crate::usb_hub::OverloadedPort;
use crate::WatchedTasksBuilder;

#[cfg(feature = "demo_mode")]
mod paths {
    pub(super) const VAR_RUN_TACD: &str = "demo_files/var/run/tacd";
    pub(super) const ETC: &str = "demo_files/etc";
}

#[cfg(not(feature = "demo_mode"))]
mod paths {
    pub(super) const VAR_RUN_TACD: &str = "/var/run/tacd";
    pub(super) const ETC: &str = "/etc";
}

use paths::*;

#[derive(Clone, Serialize, Deserialize)]
struct MotdContent {
    dut_pwr_state: OutputState,
    iobus_fault: bool,
    rauc_should_reboot: bool,
    rauc_should_update: bool,
    setup_mode_active: bool,
    temperature_warning: bool,
    usb_overload: Option<OverloadedPort>,
}

impl MotdContent {
    fn write(&self, dst: &mut File) -> std::io::Result<()> {
        writeln!(dst, "Welcome to you TAC!")?;
        writeln!(dst)?;

        let pos_pre = dst.stream_position()?;

        if self.temperature_warning {
            writeln!(dst, "Phew, it's hot in here!")?;
            writeln!(
                dst,
                "Your TAC is overheating, please provide proper airflow and let it cool down"
            )?;
            writeln!(dst)?;
        }

        if self.setup_mode_active {
            writeln!(dst, "Great! You can log in!")?;
            writeln!(
                dst,
                "You should now go to the web interface and complete the setup process"
            )?;
            writeln!(dst)?;
        }

        if self.rauc_should_reboot {
            writeln!(dst, "Could you squeeze in a reboot soon?")?;
            writeln!(
                dst,
                "There is a new operating system image in another slot."
            )?;
            writeln!(dst)?;
        }

        if self.rauc_should_update {
            writeln!(dst, "How about a software update?")?;
            writeln!(dst, "There is an operating system update available online.")?;
            writeln!(
                dst,
                "Use the web interface or the display and buttons to install it."
            )?;
            writeln!(dst)?;
        }

        match self.dut_pwr_state {
            OutputState::On => {
                writeln!(dst, "Just so you know: your DUT is currently powered on.")?;
                writeln!(dst)?;
            }
            OutputState::Off | OutputState::OffFloating | OutputState::Changing => {}
            OutputState::InvertedPolarity => {
                writeln!(
                    dst,
                    "Your DUT was powered off because of an inverted polarity event."
                )?;
                writeln!(dst)?;
            }
            OutputState::OverCurrent => {
                writeln!(
                    dst,
                    "Your DUT was powered off because of an overcurrent event."
                )?;
                writeln!(dst)?;
            }
            OutputState::OverVoltage => {
                writeln!(
                    dst,
                    "Your DUT was powered off because of an overvoltage event."
                )?;
                writeln!(dst)?;
            }
            OutputState::RealtimeViolation => {
                writeln!(dst, "Your DUT was powered off because the TAC could not keep its realtime guarantees.")?;
                writeln!(dst)?;
            }
        }

        if let Some(port) = &self.usb_overload {
            let port = match port {
                OverloadedPort::Total => 0,
                OverloadedPort::Port1 => 1,
                OverloadedPort::Port2 => 2,
                OverloadedPort::Port3 => 3,
            };

            if port == 0 {
                writeln!(
                    dst,
                    "Your USB devices are drawing to much current in total."
                )?;
                writeln!(
                    dst,
                    "All connected USB devices will likely behave strangely!"
                )?;
            } else {
                writeln!(
                    dst,
                    "The USB device on port {} is drawing to much current",
                    port
                )?;
                writeln!(dst, "It will likely behave strangely!")?;
            }

            writeln!(dst)?;
        }

        if self.iobus_fault {
            writeln!(
                dst,
                "Please have a look at the IoBus. Its power supply is overloaded"
            )?;
            writeln!(dst)?;
        }

        let pos_post = dst.stream_position()?;

        if pos_pre == pos_post {
            // It looks like we did not print anything yet and you know,
            // if you have nothing to say you should say something nice.
            // Or something like that.

            writeln!(dst, "Everything is looking fine. Have a nice day!")?;
            writeln!(dst)?;
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
            rauc_should_update: false,
            setup_mode_active: false,
            temperature_warning: false,
            usb_overload: None,
        }
    }
}

/// Create a motd in a tmpfs so we can write it without harming the eMMC
fn setup_run_tacd_motd() -> Result<File> {
    let var_run_tacd = Path::new(VAR_RUN_TACD);
    let etc = Path::new(ETC);

    // Create /var/run/tacd/motd (or an equivalent in demo mode).
    create_dir_all(VAR_RUN_TACD)?;

    // "/var/run/tacd/motd" or "demo_files/var/run/tacd/motd"
    // "/etc/motd" or "demo_files/etc/motd"
    let path_runtime_motd = var_run_tacd.join("motd");
    let path_etc_motd = etc.join("motd");

    // "/etc/motd.1234"
    let path_etc_motd_tmp = {
        let pid = process::id();
        let motd_pid = format!("motd.{}", pid);

        etc.join(motd_pid)
    };

    // Create the motd file in /var/run/tacd and symlink
    // it to /etc/motd.1234.
    let runtime_motd = File::create(&path_runtime_motd)?;
    symlink(&path_runtime_motd, &path_etc_motd_tmp)?;

    // Rename /etc/motd.1234 to /etc/motd
    rename(&path_etc_motd_tmp, path_etc_motd)?;

    Ok(runtime_motd)
}

pub fn keep_updated(
    wtb: &mut WatchedTasksBuilder,
    dut_pwr: &crate::dut_power::DutPwrThread,
    iobus: &crate::iobus::IoBus,
    rauc: &crate::dbus::Rauc,
    setup_mode: &crate::setup_mode::SetupMode,
    temperatures: &crate::temperatures::Temperatures,
    usb_hub: &crate::usb_hub::UsbHub,
) -> Result<()> {
    let mut motd = setup_run_tacd_motd()?;
    let content = Topic::<MotdContent>::anonymous(None);

    // Spawn a task that accepts motd updates from an anonymous topic
    // and dumps them into the file in /var/run.
    let (mut content_events, _) = content.clone().subscribe_unbounded();
    wtb.spawn_task("motd-write-file", async move {
        while let Some(event) = content_events.next().await {
            // Throttle the writes a bit
            sleep(Duration::from_secs(1)).await;
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
    wtb.spawn_task("motd-update-rauc-should-update", async move {
        while let Some(channels) = channels_events.next().await {
            let should_update = channels.into_iter().any(|ch| {
                ch.bundle
                    .as_ref()
                    .map(|b| b.newer_than_installed)
                    .unwrap_or(false)
            });

            content_clone.modify(|prev| {
                let mut val = prev.unwrap_or_default();

                val.rauc_should_update = should_update;

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

    Ok(())
}
