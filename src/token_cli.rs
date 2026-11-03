// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2026 Pengutronix e.K.
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

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use clap::{Args, Subcommand};

use crate::auth::{Capability, Token, TokenStore};
use crate::system::Uname;

#[derive(Args, Debug, Clone)]
pub struct GenerateArgs {
    /// Time after which the token will expire, like 30s, 15m, 1h, 1d or 1y
    #[arg(long, default_value = "1d")]
    validity: String,

    /// Print a cURL command line to the web interface instead of the bare token
    #[arg(long)]
    curl: bool,

    /// Read capabilities to give to this new token
    #[arg(long, value_enum, value_delimiter = ',')]
    read: Vec<Capability>,

    /// Write capabilities to give to this new token
    #[arg(long, value_enum, value_delimiter = ',')]
    write: Vec<Capability>,

    /// Grant all capabilities to this token
    #[arg(long)]
    admin: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Generate auth tokens
    Generate(GenerateArgs),
}

fn parse_duration(mut duration_str: String) -> Result<Duration> {
    let unit = duration_str.pop();

    let multiplier = match unit {
        Some('s') => 1,
        Some('m') => 60,
        Some('h') => 60 * 60,
        Some('d') => 24 * 60 * 60,
        Some('y') => 365 * 24 * 60 * 60,
        _ => bail!("Failed to parse duration string '{duration_str}': unknown unit"),
    };

    let value: u64 = duration_str.parse().map_err(|_| {
        anyhow!("Failed to parse duration string '{duration_str}': can not parse as u64")
    })?;

    Ok(Duration::from_secs(value * multiplier))
}

fn generate(args: GenerateArgs) -> Result<()> {
    let validity = parse_duration(args.validity)?;

    let mut token = Token::new(validity);

    // Set requested capabilities
    for cap_read in args.read {
        token.capabilities_read.set(cap_read);
    }

    for cap_write in args.write {
        token.capabilities_write.set(cap_write);
    }

    if args.admin {
        token.capabilities_read.set_all();
        token.capabilities_write.set_all();
    }

    // Encrypt and print the generated token
    let store = TokenStore::open()?;
    let token_str = store.encrypt(&token);

    if args.curl {
        let hostname = Uname::get().unwrap().nodename;
        println!("curl -H 'Authorization: Bearer {token_str}' http://{hostname}/");
    } else {
        println!("{token_str}");
    }

    Ok(())
}

pub fn token_cli(command: Commands) -> Result<()> {
    match command {
        Commands::Generate(args) => generate(args),
    }
}
