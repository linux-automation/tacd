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

fn handle_line_wo(line: Line, topic: Arc<Topic<bool>>) {
    let dst = line.request(LineRequestFlags::OUTPUT, 0, "tacd").unwrap();

    spawn(async move {
        let (mut src, _) = topic.subscribe_unbounded().await;

        while let Some(ev) = src.next().await {
            dst.set_value(if *ev { 1 } else { 0 }).unwrap();
        }
    });
}

fn handle_line_ro(line: Line, topic: Arc<Topic<bool>>) {
    let src = line
        .events(
            LineRequestFlags::INPUT,
            EventRequestFlags::BOTH_EDGES,
            "tacd",
        )
        .unwrap();

    spawn_blocking(move || {
        block_on(topic.set(src.get_value().unwrap() != 0));

        for ev in src {
            let state = match ev.unwrap().event_type() {
                EventType::RisingEdge => true,
                EventType::FallingEdge => false,
            };

            block_on(topic.set(state));
        }
    });
}

impl DigitalIo {
    pub async fn new(bb: &mut BrokerBuilder) -> Self {
        let dig_io = Self {
            out_0: bb.topic_rw("/v1/output/out_0/asserted", None),
            out_1: bb.topic_rw("/v1/output/out_1/asserted", None),
            uart_rx_en: bb.topic_rw("/v1/dut/uart/rx/enabled", None),
            uart_tx_en: bb.topic_rw("/v1/dut/uart/tx/enabled", None),
            iobus_pwr_en: bb.topic_rw("/v1/iobus/powered", None),
            iobus_flt_fb: bb.topic_ro("/v1/iobus/feedback/fault", None),
        };

        handle_line_wo(find_line("OUT_0").unwrap(), dig_io.out_0.clone());
        handle_line_wo(find_line("OUT_1").unwrap(), dig_io.out_1.clone());
        handle_line_wo(find_line("UART_RX_EN").unwrap(), dig_io.uart_rx_en.clone());
        handle_line_wo(find_line("UART_TX_EN").unwrap(), dig_io.uart_tx_en.clone());
        handle_line_wo(
            find_line("IOBUS_PWR_EN").unwrap(),
            dig_io.iobus_pwr_en.clone(),
        );
        handle_line_ro(
            find_line("IOBUS_FLT_FB").unwrap(),
            dig_io.iobus_flt_fb.clone(),
        );

        dig_io.out_0.set(false).await;
        dig_io.out_1.set(false).await;
        dig_io.uart_rx_en.set(true).await;
        dig_io.uart_tx_en.set(true).await;
        dig_io.iobus_pwr_en.set(true).await;

        dig_io
    }
}
