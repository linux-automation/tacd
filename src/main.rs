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

use futures_lite::future::race;

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
use ui::{Ui, UiRessources};
use usb_hub::UsbHub;
use watchdog::Watchdog;
use web::WebInterface;

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    pretty_env_logger::init();

    let mut bb = BrokerBuilder::new();

    let adc = Adc::new(&mut bb).await.unwrap();
    let dut_pwr = DutPwrThread::new(&mut bb, adc.pwr_volt.clone(), adc.pwr_curr.clone())
        .await
        .unwrap();
    let watchdog = Watchdog::new(dut_pwr.tick());

    let resources = UiRessources {
        adc,
        dbus: DbusSession::new(&mut bb).await,
        dig_io: DigitalIo::new(&mut bb),
        dut_pwr,
        iobus: IoBus::new(&mut bb),
        system: System::new(&mut bb),
        temperatures: Temperatures::new(&mut bb),
        usb_hub: UsbHub::new(&mut bb),
    };

    let mut web_interface = WebInterface::new();
    let ui = Ui::new(&mut bb, resources, &mut web_interface.server);
    bb.build(&mut web_interface.server);
    journal::serve(&mut web_interface.server);

    #[cfg(not(feature = "stub_out_fs"))]
    {
        web_interface.expose_file_rw(
            "/etc/labgrid/configuration.yaml",
            "/v1/labgrid/configuration",
        );
        web_interface.expose_file_rw("/etc/labgrid/environment", "/v1/labgrid/environment");
        web_interface.expose_file_rw("/etc/labgrid/userconfig.yaml", "/v1/labgrid/userconfig");
    }

    log::info!("Setup complete. Handling requests");

    race(race(ui.run(), web_interface.serve()), watchdog.keep_fed()).await
}
