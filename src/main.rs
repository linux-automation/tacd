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
use futures::{select, FutureExt};

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
use ui::{setup_display, Ui, UiResources};
use usb_hub::UsbHub;
use watchdog::Watchdog;

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Show a splash screen very early on
    let display = setup_display();

    // The BrokerBuilder collects topics that should be exported via the
    // MQTT/REST APIs.
    // The topics are also used to pass around data inside the tacd.
    let mut bb = BrokerBuilder::new();

    // Expose hardware on the TAC via the broker framework.
    let backlight = Backlight::new(&mut bb)?;
    let led = Led::new(&mut bb)?;
    let adc = Adc::new(&mut bb).await?;
    let dut_pwr = DutPwrThread::new(
        &mut bb,
        adc.pwr_volt.clone(),
        adc.pwr_curr.clone(),
        led.dut_pwr.clone(),
    )
    .await?;
    let dig_io = DigitalIo::new(&mut bb, led.out_0.clone(), led.out_1.clone())?;
    let regulators = Regulators::new(&mut bb);
    let temperatures = Temperatures::new(&mut bb);
    let usb_hub = UsbHub::new(
        &mut bb,
        adc.usb_host_curr.fast.clone(),
        adc.usb_host1_curr.fast.clone(),
        adc.usb_host2_curr.fast.clone(),
        adc.usb_host3_curr.fast.clone(),
    );

    // Expose other software on the TAC via the broker framework by connecting
    // to them via HTTP / DBus APIs.
    let iobus = IoBus::new(
        &mut bb,
        regulators.iobus_pwr_en.clone(),
        adc.iobus_curr.fast.clone(),
        adc.iobus_volt.fast.clone(),
    );
    let (network, rauc, systemd) = {
        let dbus = DbusSession::new(&mut bb, led.eth_dut.clone(), led.eth_lab.clone()).await?;

        (dbus.network, dbus.rauc, dbus.systemd)
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
    let mut http_server = HttpServer::new()?;

    // Allow editing some aspects of the TAC configuration when in "setup mode".
    let setup_mode = SetupMode::new(&mut bb, &mut http_server.server);

    // Expose a live log of the TAC's systemd journal so it can be viewed
    // in the web interface.
    journal::serve(&mut http_server.server);

    // Expose the display as a .png on the web server
    ui::serve_display(&mut http_server.server, display.screenshooter());

    // Set up the user interface for the hardware display on the TAC.
    // The different screens receive updates via the topics provided in
    // the UiResources struct.
    let ui = {
        let resources = UiResources {
            adc,
            backlight,
            dig_io,
            dut_pwr,
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

        Ui::new(&mut bb, resources)
    };

    // Consume the BrokerBuilder (no further topics can be added or removed)
    // and expose the topics via HTTP and MQTT-over-websocket.
    bb.build(&mut http_server.server);

    log::info!("Setup complete. Handling requests");

    // Run until the user interface, http server or (if selected) the watchdog
    // exits (with an error).
    if let Some(watchdog) = watchdog {
        select! {
            ui_err = ui.run(display).fuse() => ui_err,
            wi_err = http_server.serve().fuse() => wi_err,
            wd_err = watchdog.keep_fed().fuse() => wd_err,
        }?
    } else {
        select! {
            ui_err = ui.run(display).fuse() => ui_err,
            wi_err = http_server.serve().fuse() => wi_err,
        }?
    }

    Ok(())
}
