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

use futures::{select, FutureExt};

mod adc;
mod broker;
mod dbus;
mod digital_io;
mod dut_power;
mod iobus;
mod journal;
mod measurement;
mod system;
mod temperatures;
mod ui;
mod usb_hub;
mod watchdog;
mod web;

use adc::Adc;
use broker::BrokerBuilder;
use dbus::DbusSession;
use digital_io::DigitalIo;
use dut_power::DutPwrThread;
use iobus::IoBus;
use system::System;
use temperatures::Temperatures;
use ui::{Ui, UiResources};
use usb_hub::UsbHub;
use watchdog::Watchdog;
use web::WebInterface;

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    pretty_env_logger::init();

    // The BrokerBuilder collects topics that should be exported via the
    // MQTT/REST APIs.
    // The topics are also used to pass around data inside the tacd.
    let mut bb = BrokerBuilder::new();

    // Expose hardware on the TAC via the broker framework.
    let adc = Adc::new(&mut bb).await.unwrap();
    let dut_pwr = DutPwrThread::new(&mut bb, adc.pwr_volt.clone(), adc.pwr_curr.clone())
        .await
        .unwrap();
    let dig_io = DigitalIo::new(&mut bb);
    let temperatures = Temperatures::new(&mut bb);
    let usb_hub = UsbHub::new(&mut bb);

    // Expose other software on the TAC via the broker framework by connecting
    // to them via HTTP / DBus APIs.
    let iobus = IoBus::new(&mut bb);
    let (network, rauc, systemd) = {
        let dbus = DbusSession::new(&mut bb).await;

        (dbus.network, dbus.rauc, dbus.systemd)
    };

    // Expose information about the system provided by the kernel via the
    // broker framework.
    let system = System::new(&mut bb);

    // Make sure the ADC and power switching threads of the tacd are not
    // stalled for too long by providing watchdog events to systemd
    // (if requested on start).
    let watchdog = Watchdog::new(dut_pwr.tick());

    // Set up a web server and provide some static files like the web
    // interface and config files that may be edited inside the web ui.
    let mut web_interface = WebInterface::new();

    // Expose a live log of the TAC's systemd journal so it can be viewed
    // in the web interface.
    journal::serve(&mut web_interface.server);

    // Set up the user interface for the hardware display on the TAC.
    // The different screens receive updates via the topics provided in
    // the UiResources struct.
    let ui = {
        let resources = UiResources {
            adc,
            network,
            rauc,
            systemd,
            dig_io,
            dut_pwr,
            iobus,
            system,
            temperatures,
            usb_hub,
        };

        Ui::new(&mut bb, resources, &mut web_interface.server)
    };

    // Consume the BrokerBuilder (no further topics can be added or removed)
    // and expose the topics via HTTP and MQTT-over-websocket.
    bb.build(&mut web_interface.server);

    log::info!("Setup complete. Handling requests");

    // Run until the web server, user interface or (if selected) the watchdog
    // exits (with an error).
    if let Some(watchdog) = watchdog {
        select! {
            ui_err = ui.run().fuse() => ui_err,
            wi_err = web_interface.serve().fuse() => wi_err,
            wd_err = watchdog.keep_fed().fuse() => wd_err,
        }
    } else {
        select! {
            ui_err = ui.run().fuse() => ui_err,
            wi_err = web_interface.serve().fuse() => wi_err,
        }
    }
}
