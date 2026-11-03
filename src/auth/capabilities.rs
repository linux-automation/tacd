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

use clap::ValueEnum;
use serde::Serialize;

#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize)]
#[serde(into = "CapabilitySetSer")]
pub struct CapabilitySet(u16);

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Capability {
    TacSystem = 0,
    DutPower = 1,
    Outputs = 2,
    Uart = 3,
    Usb = 4,
    IoBus = 5,
}

impl Capability {
    fn mask(self) -> u16 {
        1 << (self as usize)
    }
}

impl CapabilitySet {
    pub fn empty() -> Self {
        Self::from_bits(0)
    }

    pub fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    pub fn bits(&self) -> u16 {
        self.0
    }

    pub fn is_set(&self, cap: Capability) -> bool {
        (self.0 & cap.mask()) != 0
    }

    pub fn set(&mut self, cap: Capability) {
        self.0 |= cap.mask();
    }

    pub fn set_all(&mut self) {
        self.0 = 0xffff;
    }
}

#[derive(Serialize)]
struct CapabilitySetSer {
    tac_system: bool,
    dut_power: bool,
    outputs: bool,
    uart: bool,
    usb: bool,
    iobus: bool,
}

impl From<CapabilitySet> for CapabilitySetSer {
    fn from(set: CapabilitySet) -> Self {
        Self {
            tac_system: set.is_set(Capability::TacSystem),
            dut_power: set.is_set(Capability::DutPower),
            outputs: set.is_set(Capability::Outputs),
            uart: set.is_set(Capability::Uart),
            usb: set.is_set(Capability::Usb),
            iobus: set.is_set(Capability::IoBus),
        }
    }
}
