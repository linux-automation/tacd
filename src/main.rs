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
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use anyhow::Result;
use async_std::future::pending;
use log::{error, info};

mod adc;
mod backlight;
mod broker;
mod dbus;
mod digital_io;
mod dut_power;
mod http_server;
mod iobus;
mod journal;
mod led;
mod measurement;
mod regulators;
mod setup_mode;
mod system;
mod temperatures;
mod ui;
mod usb_hub;
mod watchdog;
mod watched_tasks;

use adc::Adc;
use backlight::Backlight;
use broker::BrokerBuilder;
use dbus::DbusSession;
use digital_io::DigitalIo;
use dut_power::DutPwrThread;
use http_server::HttpServer;
use iobus::IoBus;
use led::Led;
use regulators::Regulators;
use setup_mode::SetupMode;
use system::System;
use temperatures::Temperatures;
use ui::{message, setup_display, ScreenShooter, Ui, UiResources};
use usb_hub::UsbHub;
use watchdog::Watchdog;
use watched_tasks::WatchedTasksBuilder;

async fn init(screenshooter: ScreenShooter) -> Result<(Ui, WatchedTasksBuilder)> {
    // The tacd spawns a couple of async tasks that should run as long as
    // the tacd runs and if any one fails the tacd should stop.
    // These tasks are spawned via the watched task builder.
    let mut wtb = WatchedTasksBuilder::new();

    // The BrokerBuilder collects topics that should be exported via the
    // MQTT/REST APIs.
    // The topics are also used to pass around data inside the tacd.
    let mut bb = BrokerBuilder::new();

    // Expose hardware on the TAC via the broker framework.
    let backlight = Backlight::new(&mut bb, &mut wtb)?;
    let led = Led::new(&mut bb, &mut wtb)?;
    let adc = Adc::new(&mut bb, &mut wtb).await?;
    let dut_pwr = DutPwrThread::new(
        &mut bb,
        &mut wtb,
        adc.pwr_volt.clone(),
        adc.pwr_curr.clone(),
        led.dut_pwr.clone(),
    )
    .await?;
    let dig_io = DigitalIo::new(&mut bb, &mut wtb, led.out_0.clone(), led.out_1.clone())?;
    let regulators = Regulators::new(&mut bb, &mut wtb)?;
    let temperatures = Temperatures::new(&mut bb, &mut wtb)?;
    let usb_hub = UsbHub::new(
        &mut bb,
        &mut wtb,
        adc.usb_host_curr.fast.clone(),
        adc.usb_host1_curr.fast.clone(),
        adc.usb_host2_curr.fast.clone(),
        adc.usb_host3_curr.fast.clone(),
    )?;

    // Expose other software on the TAC via the broker framework by connecting
    // to them via HTTP / DBus APIs.
    let iobus = IoBus::new(
        &mut bb,
        &mut wtb,
        regulators.iobus_pwr_en.clone(),
        adc.iobus_curr.fast.clone(),
        adc.iobus_volt.fast.clone(),
    )?;
    let (hostname, network, rauc, systemd) = {
        let dbus =
            DbusSession::new(&mut bb, &mut wtb, led.eth_dut.clone(), led.eth_lab.clone()).await?;

        (dbus.hostname, dbus.network, dbus.rauc, dbus.systemd)
    };

    // Expose information about the system provided by the kernel via the
    // broker framework.
    let system = System::new(&mut bb);

    // Make sure the ADC and power switching threads of the tacd are not
    // stalled for too long by providing watchdog events to systemd
    // (if requested on start).
    let watchdog = Watchdog::new(dut_pwr.tick());

    // Set up a http server and provide some static files like the web
    // interface and config files that may be edited inside the web ui.
    let mut http_server = HttpServer::new();

    // Allow editing some aspects of the TAC configuration when in "setup mode".
    let setup_mode = SetupMode::new(&mut bb, &mut wtb, &mut http_server.server)?;

    // Expose a live log of the TAC's systemd journal so it can be viewed
    // in the web interface.
    journal::serve(&mut http_server.server);

    // Set up the user interface for the hardware display on the TAC.
    // The different screens receive updates via the topics provided in
    // the UiResources struct.
    let ui = {
        let resources = UiResources {
            adc,
            backlight,
            dig_io,
            dut_pwr,
            hostname,
            iobus,
            led,
            network,
            rauc,
            regulators,
            setup_mode,
            system,
            systemd,
            temperatures,
            usb_hub,
        };

        Ui::new(&mut bb, &mut wtb, resources)?
    };

    // Consume the BrokerBuilder (no further topics can be added or removed)
    // and expose the topics via HTTP and MQTT-over-websocket.
    bb.build(&mut wtb, &mut http_server.server)?;

    // Expose the display as a .png on the web server
    ui::serve_display(&mut http_server.server, screenshooter);

    // Start serving files and the API
    http_server.serve(&mut wtb)?;

    // If a watchdog was requested by systemd we can now start feeding it
    if let Some(watchdog) = watchdog {
        watchdog.keep_fed(&mut wtb)?;
    }

    Ok((ui, wtb))
}

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Show a splash screen very early on
    let display = setup_display();

    // This allows us to expose screenshoots of the LCD screen via HTTP
    let screenshooter = display.screenshooter();

    match init(screenshooter).await {
        Ok((ui, mut wtb)) => {
            // Start drawing the UI
            ui.run(&mut wtb, display)?;

            info!("Setup complete. Handling requests");

            wtb.watch().await
        }
        Err(e) => {
            // Display a detailed error message on stderr (and thus in the journal) ...
            error!("Failed to initialize tacd: {e}");

            // ... and a generic message on the LCD, as it can not fit a lot of detail.
            display.clear();
            display.with_lock(|target| {
                message(
                    target,
                    "tacd failed to start!\n\nCheck log for info.\nWaiting for watchdog\nto restart tacd.",
                );
            });

            // Wait forever (or more likely until the systemd watchdog timer hits)
            // to give the user a chance to actually see the error message.
            pending().await
        }
    }
}
