use futures_lite::future::race;

mod adc;
mod broker;
mod dbus;
mod digital_io;
mod dut_power;
mod iobus;
mod system;
mod temperatures;
mod ui;
mod usb_hub;
mod watchdog;
mod web;

use adc::Adc;
use broker::BrokerBuilder;
use dbus::DbusClient;
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
    let mut bb = BrokerBuilder::new();

    let adc = Adc::new(&mut bb);
    let dut_pwr = DutPwrThread::new(&mut bb, adc.pwr_curr.clone(), adc.pwr_volt.clone());
    let watchdog = Watchdog::new(dut_pwr.tick());

    let ressources = UiRessources {
        adc,
        dbus: DbusClient::new(&mut bb).await,
        dig_io: DigitalIo::new(&mut bb),
        dut_pwr,
        iobus: IoBus::new(&mut bb),
        system: System::new(&mut bb),
        temperatures: Temperatures::new(&mut bb),
        usb_hub: UsbHub::new(&mut bb),
    };

    let mut web_interface = WebInterface::new();
    let ui = Ui::new(&mut bb, ressources, &mut web_interface.server);
    bb.build(&mut web_interface.server);

    web_interface.expose_file_rw(
        "/etc/labgrid/configuration.yaml",
        "/v1/labgrid/configuration",
    );
    web_interface.expose_file_rw("/etc/labgrid/environment", "/v1/labgrid/environment");
    web_interface.expose_file_rw("/etc/labgrid/userconfig.yaml", "/v1/labgrid/userconfig");

    race(race(ui.run(), web_interface.serve()), watchdog.keep_fed()).await
}
