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

use anyhow::Result;
use clap::{self, Parser};

mod adc;
mod backlight;
mod broker;
mod daemon;
mod dbus;
mod digital_io;
mod dut_power;
mod http_server;
mod inhibit;
mod iobus;
mod journal;
mod led;
mod measurement;
mod motd;
mod regulators;
mod selftest;
mod setup_mode;
mod system;
mod temperatures;
mod ui;
mod usb_hub;
mod watchdog;
mod watched_tasks;

#[derive(clap::Parser, Debug)]
#[command(name = "tacd")]
#[command(author, version, about)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommands>,
}

#[derive(clap::Subcommand, Debug)]
enum CliCommands {
    /// Start the tacd service
    Daemon,
    /// Helper to test LXATAC functions.
    Selftest {
        #[command(subcommand)]
        tests: selftest::Commands,
    },
}

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command.unwrap_or(CliCommands::Daemon) {
        CliCommands::Daemon => daemon::daemon().await,
        CliCommands::Selftest { tests } => selftest::selftests(tests).await,
    }
}
