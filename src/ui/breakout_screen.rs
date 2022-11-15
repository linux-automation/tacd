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

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{sleep, spawn, JoinHandle};
use async_trait::async_trait;

use crate::broker::{Native, SubscriptionHandle};
use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

use super::widgets::UI_TEXT_FONT;
use super::{ButtonEvent, MountableScreen, Screen, Ui};

// The first rule of the the breakout easteregg screen is:
// we do not talk about the breakout easteregg screen
// (at least in public or in larger groups (this includes IRC)).
// Keep it fun, eh?

const SCREEN_TYPE: Screen = Screen::Breakout;

const LEVELS: &[u8] = br#"
    ############
    ############
    #.##.#.##.##
    #.###.##.#.#
    #.##.#.#...#
    #..#.#.#.#.#
    ############
    ############

    ############
    ############
    #...#.###..#
    ##.#.#.#.###
    ##.#...#.###
    ##.#.#.##..#
    ############
    ############

    .###########
    .###########
    .##..###..##
    .#.........#
    .##......###
    .###....####
    .#####.#####
    .###########

    .##########.
    ##..####..##
    ##..####..##
    ############
    ############
    ##.######.##
    ###......###
    .##########.

    ############
    ############
    ############
    ############
    ############
    ############
    ############
    ############

    ###.........
    ###.........
    ###.........
    ###.........
    ###.........
    ###.........
    ###.........
    ###.........
"#;

pub struct BreakoutScreen {
    buttons_handle: Option<SubscriptionHandle<ButtonEvent, Native>>,
    join_handle: Option<JoinHandle<()>>,
}

fn block_bb(x: u8, y: u8) -> Rectangle {
    Rectangle::with_center(
        Point::new((x as i32) * 8 + 36, (y as i32) * 8 + 4),
        Size::new(6, 6),
    )
}

enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

fn collision_side(rect: &Rectangle, circ: &Circle) -> Option<Side> {
    let is = rect.intersection(&circ.bounding_box());

    if is.is_zero_sized() {
        None
    } else {
        let Point { x, y } = is.center() - rect.center();

        match (x.abs() > y.abs(), x > 0, y > 0) {
            (true, true, _) => Some(Side::Right),
            (true, false, _) => Some(Side::Left),
            (false, _, true) => Some(Side::Top),
            (false, _, false) => Some(Side::Bottom),
        }
    }
}

fn to_idx(x: usize, y: usize) -> usize {
    y * 12 + x
}

impl BreakoutScreen {
    pub fn new() -> Self {
        Self {
            join_handle: None,
            buttons_handle: None,
        }
    }
}

#[async_trait]
impl MountableScreen for BreakoutScreen {
    fn is_my_type(&self, screen: Screen) -> bool {
        screen == SCREEN_TYPE
    }

    async fn mount(&mut self, ui: &Ui) {
        let (paddle_y_in, paddle_y_out) = {
            let p = Arc::new(AtomicI32::new(32));
            (p.clone(), p)
        };

        let draw_target = ui.draw_target.clone();

        let join_handle = spawn(async move {
            let mut levels: Vec<bool> = LEVELS
                .iter()
                .filter_map(|s| match s {
                    b'#' => Some(true),
                    b'.' => Some(false),
                    _ => None,
                })
                .collect();

            for level in levels.chunks_exact_mut(12 * 8) {
                let draw_style = PrimitiveStyle::with_fill(BinaryColor::On);
                let clear_style = PrimitiveStyle::with_fill(BinaryColor::Off);

                for x in 0..12u8 {
                    for y in 0..8u8 {
                        if level[to_idx(x as usize, y as usize)] {
                            block_bb(x, y)
                                .into_styled(draw_style)
                                .draw(&mut *draw_target.lock().await)
                                .unwrap()
                        }
                    }
                }

                let (mut bx, mut by, mut sx, mut sy) = (32, 32, 1, 1);

                let mut done = false;

                while !done {
                    let py = paddle_y_out.load(Ordering::Relaxed);

                    bx += sx;
                    by += sy;

                    if bx < 7 {
                        if (by - py).abs() > 12 {
                            // Paddle miss. Reset
                            (bx, by, sx, sy) = (32, 32, 1, 1);
                        } else {
                            bx = 7;
                            sx = 1;
                            sy = if (by - py).abs() < 4 {
                                0
                            } else {
                                if by < py {
                                    -1
                                } else {
                                    1
                                }
                            };
                        }
                    }

                    if bx > 123 {
                        bx = 123;
                        sx = -1;
                    }

                    if by < 5 {
                        by = 5;
                        sy = 1;
                    }

                    if by > 59 {
                        by = 59;
                        sy = -1;
                    }

                    let ball = Circle::with_center(Point::new(bx, by), 8);

                    done = true;

                    for x in 0..12u8 {
                        for y in 0..8u8 {
                            if !level[to_idx(x as usize, y as usize)] {
                                continue;
                            }

                            done = false;

                            let bb = block_bb(x, y);

                            if let Some(side) = collision_side(&bb, &ball) {
                                match side {
                                    Side::Top => sy = 1,
                                    Side::Bottom => sy = -1,
                                    Side::Left => sx = -1,
                                    Side::Right => sx = 1,
                                };

                                level[to_idx(x as usize, y as usize)] = false;
                                bb.into_styled(clear_style)
                                    .draw(&mut *draw_target.lock().await)
                                    .unwrap();
                            }
                        }
                    }

                    let paddle = Rectangle::with_center(Point::new(3, py), Size::new(2, 20));
                    ball.into_styled(draw_style)
                        .draw(&mut *draw_target.lock().await)
                        .unwrap();
                    paddle
                        .into_styled(draw_style)
                        .draw(&mut *draw_target.lock().await)
                        .unwrap();

                    sleep(Duration::from_millis(60)).await;

                    ball.into_styled(clear_style)
                        .draw(&mut *draw_target.lock().await)
                        .unwrap();
                    paddle
                        .into_styled(clear_style)
                        .draw(&mut *draw_target.lock().await)
                        .unwrap();
                }
            }

            let mut draw_target = draw_target.lock().await;
            let text_style: MonoTextStyle<BinaryColor> =
                MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);
            Text::with_alignment(
                "Well done!\nYou may want to\ngo back to work now",
                Point::new(64, 25),
                text_style,
                Alignment::Center,
            )
            .draw(&mut *draw_target)
            .unwrap();
        });

        let (mut button_events, buttons_handle) = ui.buttons.clone().subscribe_unbounded().await;
        spawn(async move {
            while let Some(ev) = button_events.next().await {
                match *ev {
                    ButtonEvent::ButtonOne(_) => paddle_y_in.fetch_add(4, Ordering::Relaxed),
                    ButtonEvent::ButtonTwo(_) => paddle_y_in.fetch_sub(4, Ordering::Relaxed),
                };
            }
        });

        self.join_handle = Some(join_handle);
        self.buttons_handle = Some(buttons_handle);
    }

    async fn unmount(&mut self) {
        if let Some(handle) = self.buttons_handle.take() {
            handle.unsubscribe().await;
        }
        if let Some(handle) = self.join_handle.take() {
            handle.cancel().await;
        }
    }
}
