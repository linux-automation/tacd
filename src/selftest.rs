use anyhow::Result;
use clap::Subcommand;

use crate::broker::BrokerBuilder;
use crate::system::HardwareGeneration;
use crate::watched_tasks::WatchedTasksBuilder;

mod adc;
mod ui;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Sample ADC channels and calculate statistic
    Adc(adc::AdcArgs),
    UiTest,
}

pub async fn selftests(command: Commands) -> Result<()> {
    // The tacd spawns a couple of async tasks that should run as long as
    // the tacd runs and if any one fails the tacd should stop.
    // These tasks are spawned via the watched task builder.
    let wtb = WatchedTasksBuilder::new();

    // The BrokerBuilder collects topics that should be exported via the
    // MQTT/REST APIs.
    // The topics are also used to pass around data inside the tacd.
    let bb = BrokerBuilder::new();

    let hardware_generation = HardwareGeneration::get()?;

    match command {
        Commands::Adc(adc_args) => {
            adc::collect_adc_samples(bb, wtb, hardware_generation, adc_args).await
        }
        Commands::UiTest => ui::ui_test(bb, wtb).await,
    }
}
