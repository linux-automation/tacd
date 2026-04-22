use std::time::Duration;

use anyhow::{Result, anyhow};
use async_std::channel::Receiver;
use async_std::stream::StreamExt;
use futures::FutureExt;

use crate::broker::Topic;
use crate::led::{
    BlinkPattern, BlinkPatternBuilder, Brightness, Pattern, RgbColor, get_led_checked,
};
use crate::ui::{Button, ButtonEvent, Direction, handle_buttons};
use crate::{broker::BrokerBuilder, watched_tasks::WatchedTasksBuilder};

const LIST_LEDS: [&str; 14] = [
    "tac:green:statusdut",
    "tac:green:dutpwr",
    "tac:green:user1",
    "tac:green:user2",
    "tac:green:can",
    "tac:green:iobus",
    "tac:green:out0",
    "tac:green:out1",
    "tac:green:uarttx",
    "tac:green:uartrx",
    "tac:green:usbg",
    "tac:green:usbh1",
    "tac:green:usbh2",
    "tac:green:usbh3",
];

fn set_all_leds() -> Result<()> {
    println!("Setup LEDs");
    let blink_pattern = BlinkPatternBuilder::new(1.0)
        .stay_for(Duration::from_millis(500))
        .step_to(0.0)
        .stay_for(Duration::from_millis(500))
        .forever();

    for name in LIST_LEDS.iter() {
        println!(" - {}", name);
        let led = get_led_checked(name).ok_or(anyhow!("No LED with name {}", name))?;
        led.set_pattern(blink_pattern.clone())?;
    }
    // Ethernet lab side
    // This is a bi-color LED. So we need to swap polarity to see booth colors.
    // /-----\_____/
    // ___/-----\___
    let led_o = get_led_checked("tac:orange:statuslab")
        .ok_or(anyhow::anyhow!("No LED with name tac:orange:statuslab"))?;
    let led_g = get_led_checked("tac:green:statuslab")
        .ok_or(anyhow::anyhow!("No LED with name tac:green:statuslab"))?;
    led_o.set_pattern(
        BlinkPatternBuilder::new(1.0)
            .stay_for(Duration::from_millis(1000))
            .step_to(0.0)
            .stay_for(Duration::from_millis(1000))
            .forever(),
    )?;
    led_g.set_pattern(
        BlinkPatternBuilder::new(0.0)
            .stay_for(Duration::from_millis(500))
            .step_to(1.0)
            .stay_for(Duration::from_millis(1000))
            .step_to(0.0)
            .stay_for(Duration::from_millis(500))
            .forever(),
    )?;
    Ok(())
}

fn clear_all_leds() -> Result<()> {
    println!("Turn off LEDs");
    let blink_pattern = BlinkPattern::solid(0.0);

    for name in LIST_LEDS.iter() {
        println!(" - {}", name);
        let led = get_led_checked(name).ok_or(anyhow!("No LED with name {}", name))?;
        led.set_pattern(blink_pattern.clone())?;
    }
    // Ethernet lab side
    // Needed special setup so not in LIST_LEDS
    let led_o = get_led_checked("tac:orange:statuslab")
        .ok_or(anyhow::anyhow!("No LED with name tac:orange:statuslab"))?;
    let led_g = get_led_checked("tac:green:statuslab")
        .ok_or(anyhow::anyhow!("No LED with name tac:green:statuslab"))?;
    led_o.set_pattern(blink_pattern.clone())?;
    led_g.set_pattern(blink_pattern.clone())?;
    Ok(())
}

fn set_rgb_led(red: bool, green: bool, blue: bool) -> Result<()> {
    let led_rgb = get_led_checked("rgb:status").unwrap();
    let max_brightness = led_rgb.max_brightness().unwrap();
    led_rgb
        .set_rgb_color(
            max_brightness * (red as u64),
            max_brightness * (green as u64),
            max_brightness * (blue as u64),
        )
        .unwrap();
    Ok(())
}

async fn wait_button_press(button_events: &mut Receiver<ButtonEvent>) -> Result<u8> {
    while let Some(ev) = button_events.next().fuse().await {
        println!("{:?}", ev);
        match (ev.dir, ev.btn) {
            (Direction::Press, Button::Lower) => return Ok(0b01),
            (Direction::Press, Button::Upper) => return Ok(0b10),
            _ => {}
        }
    }
    anyhow::bail!("Button event queue ended unexpectedly")
}

pub async fn ui_test(mut _bb: BrokerBuilder, mut wtb: WatchedTasksBuilder) -> Result<()> {
    let buttons = Topic::anonymous(None);
    handle_buttons(
        &mut wtb,
        "/dev/input/by-path/platform-gpio-keys-event",
        buttons.clone(),
    )?;
    let (mut button_events, _) = buttons.clone().subscribe_unbounded();

    set_all_leds()?;
    set_rgb_led(true, true, true)?;

    let mut colors = [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ]
    .into_iter()
    .cycle();

    let mut buttons_seen: u8 = 0;
    loop {
        buttons_seen |= wait_button_press(&mut button_events).await?;
        if buttons_seen == 0b11 {
            break;
        }
        let color = colors.next().unwrap();
        set_rgb_led(color.0, color.1, color.2)?;
    }

    clear_all_leds()?;
    set_rgb_led(false, false, false)?;
    Ok(())
}
