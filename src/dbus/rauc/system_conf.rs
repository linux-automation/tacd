use std::fmt::Write;
use std::fs::{create_dir_all, read_to_string, remove_file, rename, write};
use std::io::{Error, ErrorKind};
use std::path::Path;

use super::Channel;

use log::info;

#[cfg(feature = "demo_mode")]
mod imports {
    pub(super) const STATIC_CONF_PATH: &str = "demo_files/usr/lib/rauc/system.conf";
    pub(super) const DYNAMIC_CONF_PATH: &str = "demo_files/run/rauc/system.conf";
}

#[cfg(not(feature = "demo_mode"))]
mod imports {
    pub(super) const STATIC_CONF_PATH: &str = "/usr/lib/rauc/system.conf";
    pub(super) const DYNAMIC_CONF_PATH: &str = "/run/rauc/system.conf";
}

use imports::*;

const MAGIC_LINE: &str = "\n# <tacd-poll-section>\n";

fn poll_section(
    primary_channel: Option<&Channel>,
    polling: bool,
    auto_install: bool,
) -> Result<Option<String>, std::fmt::Error> {
    // If no primary channel is configured or if polling is not enabled,
    // then we do not need a `[poll]` section at all.
    let primary_channel = match (primary_channel, polling) {
        (Some(pc), true) => pc,
        _ => return Ok(None),
    };

    let mut section = String::new();

    writeln!(&mut section)?;
    writeln!(&mut section, "[poll]")?;
    writeln!(&mut section, "source={}", primary_channel.url)?;

    if let Some(interval) = primary_channel.polling_interval {
        writeln!(&mut section, "interval-sec={}", interval.as_secs())?;
    }

    let candidate_criteria = primary_channel
        .candidate_criteria
        .as_deref()
        .unwrap_or("different-version");

    writeln!(&mut section, "candidate-criteria={candidate_criteria}")?;

    if auto_install {
        let install_criteria = primary_channel
            .install_criteria
            .as_deref()
            .unwrap_or("different-version");
        let reboot_criteria = primary_channel
            .reboot_criteria
            .as_deref()
            .unwrap_or("updated-slots;updated-artifacts");

        writeln!(&mut section, "install-criteria={install_criteria}")?;
        writeln!(&mut section, "reboot-criteria={reboot_criteria}")?;
        writeln!(&mut section, "reboot-cmd=systemctl reboot")?;
    }

    Ok(Some(section))
}

pub fn update_system_conf(
    primary_channel: Option<&Channel>,
    enable_polling: bool,
    enable_auto_install: bool,
    setup_mode: bool,
) -> std::io::Result<bool> {
    let dynamic_conf = {
        // Allow force-enabling update polling and automatic installations
        // via the update channel config file to implement company wide
        // auto-update policies.
        let force_polling = primary_channel
            .and_then(|pc| pc.force_polling)
            .unwrap_or(false);
        let force_auto_install = primary_channel
            .and_then(|pc| pc.force_auto_install)
            .unwrap_or(false);

        // Allow polling for updates, but not automatically installing them
        // while the user is still in setup mode.
        // Otherwise they may unbox a TAC, click through the setup process,
        // activate auto installation, and then an installation starts in the
        // background without them even noticing.
        let polling = enable_polling || force_polling;
        let auto_install = (enable_auto_install || force_auto_install) && !setup_mode;

        match poll_section(primary_channel, polling, auto_install) {
            Ok(Some(ps)) => {
                // We use the config in /etc as a template ...
                let static_conf = read_to_string(STATIC_CONF_PATH)?;

                // ... and replace the line `# <tacd-poll-section>` with our
                // generated `[poll]` section.
                let dc = static_conf.replacen(MAGIC_LINE, &ps, 1);

                // The user may have decided not to include a `# <tacd-poll-section>`
                // line. In that case we do not need a dynamic config at all.
                if dc == static_conf {
                    info!(
                        "Rauc config {} did not contain magic line '{}'. Will not generate poll section.",
                        STATIC_CONF_PATH, MAGIC_LINE
                    );

                    None
                } else {
                    Some(dc)
                }
            }
            _ => None,
        }
    };

    /* Do we need a dynamic config in /run/rauc?
     *
     * If so, then is it actually different from what we already have?
     * If not, then there is no need to restart the daemon.
     * If it is, we write the new one and signal the need for a daemon
     * restart.
     *
     * If we do not need dynamic config, then try to delete the previous one.
     * If there was none, then the daemon does not have to be restarted.
     * If there was a dynamic config before, then we need to restart the
     * daemon.
     */
    match dynamic_conf {
        Some(new) => match read_to_string(DYNAMIC_CONF_PATH) {
            Ok(old) if old == new => Ok(false),
            Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
            Ok(_) | Err(_) => {
                let dynamic_conf_dir = Path::new(DYNAMIC_CONF_PATH)
                    .parent()
                    .ok_or_else(|| Error::other("Invalid dynamic config path"))?;

                let tmp_path = dynamic_conf_dir.join("system.conf.tacd-tmp");

                create_dir_all(dynamic_conf_dir)?;

                write(&tmp_path, &new)?;
                rename(&tmp_path, DYNAMIC_CONF_PATH)?;

                Ok(true)
            }
        },
        None => match remove_file(DYNAMIC_CONF_PATH) {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        },
    }
}
