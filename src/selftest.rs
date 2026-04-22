use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Commands {}

pub fn selftests(cli: Commands) -> Result<()> {
    println!("{:?}", cli);
    Ok(())
}
