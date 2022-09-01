use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{block_on, spawn, spawn_blocking};
use gpio_cdev::{chips, EventRequestFlags, EventType, Line, LineRequestFlags};

use crate::broker::{BrokerBuilder, Topic};

pub struct DigitalIo {
    pub out_0: Arc<Topic<bool>>,
    pub out_1: Arc<Topic<bool>>,
    pub uart_rx_en: Arc<Topic<bool>>,
    pub uart_tx_en: Arc<Topic<bool>>,
    pub iobus_pwr_en: Arc<Topic<bool>>,
    pub iobus_flt_fb: Arc<Topic<bool>>,
}

pub fn find_line(name: &str) -> Option<Line> {
    chips()
        .unwrap()
        .flat_map(|c| c.unwrap().lines())
        .find(|l| l.info().unwrap().name() == Some(name))
}

/// Handle a GPIO line whose state is completely defined by the broker framwork
/// writing to it. (e.g. whatever it is set to _is_ the line status).
fn handle_line_wo(
    bb: &mut BrokerBuilder,
    path: &str,
    line_name: &str,
    initial: bool,
) -> Arc<Topic<bool>> {
    let topic = bb.topic_rw(path, Some(initial));
    let line = find_line(line_name).unwrap();
    let dst = line
        .request(LineRequestFlags::OUTPUT, initial as _, "tacd")
        .unwrap();

    let topic_task = topic.clone();

    spawn(async move {
        let (mut src, _) = topic_task.subscribe_unbounded().await;

        while let Some(ev) = src.next().await {
            dst.set_value(*ev as _).unwrap();
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
            out_0: handle_line_wo(bb, "/v1/output/out_0/asserted", "OUT_0", false),
            out_1: handle_line_wo(bb, "/v1/output/out_1/asserted", "OUT_1", false),
            uart_rx_en: handle_line_wo(bb, "/v1/dut/uart/rx/enabled", "UART_RX_EN", true),
            uart_tx_en: handle_line_wo(bb, "/v1/dut/uart/tx/enabled", "UART_TX_EN", true),
            iobus_pwr_en: handle_line_wo(bb, "/v1/iobus/powered", "IOBUS_PWR_EN", true),
            iobus_flt_fb: handle_line_ro(bb, "/v1/iobus/feedback/fault", "IOBUS_FLT_FB"),
        }
    }
}
