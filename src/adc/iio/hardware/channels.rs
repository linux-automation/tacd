// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2024 Pengutronix e.K.
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

use crate::system::HardwareGeneration;

pub(super) struct ChannelDesc {
    pub kernel_name: &'static str,
    pub calibration_path: &'static str,
    pub name: &'static str,
}

// Hard coded list of channels using the internal STM32MP1 ADC.
// Consists of the IIO channel name, the location of the calibration data
// in the device tree and an internal name for the channel.
const CHANNELS_STM32_GEN1_GEN2: &[ChannelDesc] = &[
    ChannelDesc {
        kernel_name: "voltage13",
        calibration_path: "baseboard-factory-data/usb-host-curr",
        name: "usb-host-curr",
    },
    ChannelDesc {
        kernel_name: "voltage15",
        calibration_path: "baseboard-factory-data/usb-host1-curr",
        name: "usb-host1-curr",
    },
    ChannelDesc {
        kernel_name: "voltage0",
        calibration_path: "baseboard-factory-data/usb-host2-curr",
        name: "usb-host2-curr",
    },
    ChannelDesc {
        kernel_name: "voltage1",
        calibration_path: "baseboard-factory-data/usb-host3-curr",
        name: "usb-host3-curr",
    },
    ChannelDesc {
        kernel_name: "voltage2",
        calibration_path: "baseboard-factory-data/out0-volt",
        name: "out0-volt",
    },
    ChannelDesc {
        kernel_name: "voltage10",
        calibration_path: "baseboard-factory-data/out1-volt",
        name: "out1-volt",
    },
    ChannelDesc {
        kernel_name: "voltage5",
        calibration_path: "baseboard-factory-data/iobus-curr",
        name: "iobus-curr",
    },
    ChannelDesc {
        kernel_name: "voltage9",
        calibration_path: "baseboard-factory-data/iobus-volt",
        name: "iobus-volt",
    },
];

const CHANNELS_STM32_GEN3: &[ChannelDesc] = &[
    ChannelDesc {
        kernel_name: "voltage13",
        calibration_path: "baseboard-factory-data/usb-host-curr",
        name: "usb-host-curr",
    },
    ChannelDesc {
        kernel_name: "voltage15",
        calibration_path: "baseboard-factory-data/usb-host1-curr",
        name: "usb-host1-curr",
    },
    ChannelDesc {
        kernel_name: "voltage18",
        calibration_path: "baseboard-factory-data/usb-host2-curr",
        name: "usb-host2-curr",
    },
    ChannelDesc {
        kernel_name: "voltage14",
        calibration_path: "baseboard-factory-data/usb-host3-curr",
        name: "usb-host3-curr",
    },
    ChannelDesc {
        kernel_name: "voltage2",
        calibration_path: "baseboard-factory-data/out0-volt",
        name: "out0-volt",
    },
    ChannelDesc {
        kernel_name: "voltage10",
        calibration_path: "baseboard-factory-data/out1-volt",
        name: "out1-volt",
    },
    ChannelDesc {
        kernel_name: "voltage5",
        calibration_path: "baseboard-factory-data/iobus-curr",
        name: "iobus-curr",
    },
    ChannelDesc {
        kernel_name: "voltage9",
        calibration_path: "baseboard-factory-data/iobus-volt",
        name: "iobus-volt",
    },
];

// The same as for the STM32MP1 channels but for the discrete ADC on the power
// board.
const CHANNELS_PWR: &[ChannelDesc] = &[
    ChannelDesc {
        kernel_name: "voltage",
        calibration_path: "powerboard-factory-data/pwr-volt",
        name: "pwr-volt",
    },
    ChannelDesc {
        kernel_name: "current",
        calibration_path: "powerboard-factory-data/pwr-curr",
        name: "pwr-curr",
    },
];

pub(super) trait Channels {
    fn channels_stm32(&self) -> &'static [ChannelDesc];
    fn channels_pwr(&self) -> &'static [ChannelDesc];
}

impl Channels for HardwareGeneration {
    fn channels_stm32(&self) -> &'static [ChannelDesc] {
        // LXA TAC hardware generation 3 has move some of the ADC channels around
        // so that channel 0 and 1 are no longer used.
        // Channel 0 and 1 are special in that they do not use the pinmuxing support
        // of the STM32MP1 SoC.
        // Instead they are always connected to the ADC.
        // This causes issues when the ADC peripheral is put into stanby,
        // because current will leak into these pins in that case.

        match self {
            HardwareGeneration::Gen1 | HardwareGeneration::Gen2 => CHANNELS_STM32_GEN1_GEN2,
            HardwareGeneration::Gen3 => CHANNELS_STM32_GEN3,
        }
    }

    fn channels_pwr(&self) -> &'static [ChannelDesc] {
        // The pin assignment of the power board is currently independent from the
        // hardware generation.
        CHANNELS_PWR
    }
}
