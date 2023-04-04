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
use async_std::task::{spawn, spawn_blocking};

use crate::broker::{BrokerBuilder, Topic};
use crate::led::BlinkPattern;

#[cfg(test)]
mod gpio {
    mod test;
    pub use test::*;
}

#[cfg(feature = "demo_mode")]
mod gpio {
    mod demo_mode;
    pub use demo_mode::*;
}

#[cfg(not(any(test, feature = "demo_mode")))]
mod gpio {
    mod hardware;
    pub use hardware::*;
}

pub use gpio::{find_line, EventRequestFlags, EventType, LineHandle, LineRequestFlags};

pub struct DigitalIo {
    pub out_0: Arc<Topic<bool>>,
    pub out_1: Arc<Topic<bool>>,
    pub uart_rx_en: Arc<Topic<bool>>,
    pub uart_tx_en: Arc<Topic<bool>>,
    pub iobus_flt_fb: Arc<Topic<bool>>,
}

/// Handle a GPIO line whose state is completely defined by the broker framework
/// writing to it. (e.g. whatever it is set to _is_ the line status).
fn handle_line_wo(
    bb: &mut BrokerBuilder,
    path: &str,
    line_name: &str,
    initial: bool,
    inverted: bool,
    led_topic: Option<Arc<Topic<BlinkPattern>>>,
) -> Arc<Topic<bool>> {
    let topic = bb.topic_rw(path, Some(initial));
    let line = find_line(line_name).unwrap();
    let dst = line
        .request(LineRequestFlags::OUTPUT, (initial ^ inverted) as _, "tacd")
        .unwrap();

    let (mut src, _) = topic.clone().subscribe_unbounded();

    spawn(async move {
        while let Some(ev) = src.next().await {
            dst.set_value((ev ^ inverted) as _).unwrap();

            if let Some(led) = &led_topic {
                let pattern = BlinkPattern::solid(if ev { 1.0 } else { 0.0 });
                led.set(pattern);
            }
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
        topic_thread.set(src.get_value().unwrap() != 0);

        for ev in src {
            let state = match ev.unwrap().event_type() {
                EventType::RisingEdge => true,
                EventType::FallingEdge => false,
            };

            topic_thread.set(state);
        }
    });

    topic
}

impl DigitalIo {
    pub fn new(
        bb: &mut BrokerBuilder,
        led_0: Arc<Topic<BlinkPattern>>,
        led_1: Arc<Topic<BlinkPattern>>,
    ) -> Self {
        let out_0 = handle_line_wo(
            bb,
            "/v1/output/out_0/asserted",
            "OUT_0",
            false,
            false,
            Some(led_0),
        );

        let out_1 = handle_line_wo(
            bb,
            "/v1/output/out_1/asserted",
            "OUT_1",
            false,
            false,
            Some(led_1),
        );

        let uart_rx_en = handle_line_wo(bb, "/v1/uart/rx/enabled", "UART_RX_EN", true, true, None);
        let uart_tx_en = handle_line_wo(bb, "/v1/uart/tx/enabled", "UART_TX_EN", true, true, None);
        let iobus_flt_fb = handle_line_ro(bb, "/v1/iobus/feedback/fault", "IOBUS_FLT_FB");

        Self {
            out_0,
            out_1,
            uart_rx_en,
            uart_tx_en,
            iobus_flt_fb,
        }
    }
}
