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

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{block_on, spawn, spawn_blocking};

#[cfg(any(test, feature = "stub_out_gpio"))]
mod gpio {
    mod stub;
    pub use stub::*;
}

#[cfg(not(any(test, feature = "stub_out_gpio")))]
mod gpio {
    mod hardware;
    pub use hardware::*;
}

pub use gpio::{find_line, EventRequestFlags, EventType, LineHandle, LineRequestFlags};

use crate::broker::{BrokerBuilder, Topic};

pub struct DigitalIo {
    pub out_0: Arc<Topic<bool>>,
    pub out_1: Arc<Topic<bool>>,
    pub uart_rx_en: Arc<Topic<bool>>,
    pub uart_tx_en: Arc<Topic<bool>>,
    pub iobus_pwr_en: Arc<Topic<bool>>,
    pub iobus_flt_fb: Arc<Topic<bool>>,
}

/// Handle a GPIO line whose state is completely defined by the broker framwork
/// writing to it. (e.g. whatever it is set to _is_ the line status).
fn handle_line_wo(
    bb: &mut BrokerBuilder,
    path: &str,
    line_name: &str,
    initial: bool,
    inverted: bool,
) -> Arc<Topic<bool>> {
    let topic = bb.topic_rw(path, Some(initial ^ inverted));
    let line = find_line(line_name).unwrap();
    let dst = line
        .request(LineRequestFlags::OUTPUT, (initial ^ inverted) as _, "tacd")
        .unwrap();

    let topic_task = topic.clone();

    spawn(async move {
        let (mut src, _) = topic_task.subscribe_unbounded().await;

        while let Some(ev) = src.next().await {
            dst.set_value((*ev ^ inverted) as _).unwrap();
        }
    });

    topic
}

/// Handle a GPIO line whose state is completely defined by itself
/// (e.g. there is no way to manipulate it via the broker framework).
fn handle_line_ro(bb: &mut BrokerBuilder, path: &str, line_name: &str) -> Arc<Topic<bool>> {
    let topic = bb.topic_ro(path, None);
    let line = find_line(line_name).unwrap();

    let topic_thread = topic.clone();

    let src = line
        .events(
            LineRequestFlags::INPUT,
            EventRequestFlags::BOTH_EDGES,
            "tacd",
        )
        .unwrap();

    spawn_blocking(move || {
        block_on(topic_thread.set(src.get_value().unwrap() != 0));

        for ev in src {
            let state = match ev.unwrap().event_type() {
                EventType::RisingEdge => true,
                EventType::FallingEdge => false,
            };

            block_on(topic_thread.set(state));
        }
    });

    topic
}

impl DigitalIo {
    pub fn new(bb: &mut BrokerBuilder) -> Self {
        Self {
            out_0: handle_line_wo(bb, "/v1/output/out_0/asserted", "OUT_0", false, false),
            out_1: handle_line_wo(bb, "/v1/output/out_1/asserted", "OUT_1", false, false),
            uart_rx_en: handle_line_wo(bb, "/v1/uart/rx/enabled", "UART_RX_EN", true, true),
            uart_tx_en: handle_line_wo(bb, "/v1/uart/tx/enabled", "UART_TX_EN", true, true),
            iobus_pwr_en: handle_line_wo(bb, "/v1/iobus/powered", "IOBUS_PWR_EN", true, false),
            iobus_flt_fb: handle_line_ro(bb, "/v1/iobus/feedback/fault", "IOBUS_FLT_FB"),
        }
    }
}
