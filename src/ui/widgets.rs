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

use anyhow::anyhow;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task::{spawn, JoinHandle};
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoFont, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Text},
};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::broker::{Native, SubscriptionHandle, Topic};
use crate::ui::display::{Display, DisplayExclusive};

pub const UI_TEXT_FONT: MonoFont = FONT_10X20;

pub enum IndicatorState {
    On,
    Off,
    Error,
    Unkown,
}

pub struct WidgetContainer {
    display: Arc<Display>,
    widgets: Vec<Box<dyn AnyWidget>>,
}

impl WidgetContainer {
    pub fn new(display: Display) -> Self {
        Self {
            display: Arc::new(display),
            widgets: Vec::new(),
        }
    }

    pub fn push<F, W>(&mut self, create_fn: F)
    where
        F: FnOnce(Arc<Display>) -> W,
        W: AnyWidget + 'static,
    {
        let display = self.display.clone();
        let widget = create_fn(display);
        self.widgets.push(Box::new(widget));
    }

    pub async fn destroy(self) -> Display {
        for widget in self.widgets.into_iter() {
            widget.unmount().await;
        }

        Arc::try_unwrap(self.display)
            .map_err(|e| {
                anyhow!(
                    "Failed to re-unite display references. Have {} references instead of 1",
                    Arc::strong_count(&e)
                )
            })
            .unwrap()
    }
}

pub trait DrawFn<T>: Fn(&T, &mut DisplayExclusive) -> Option<Rectangle> {}
impl<T, U> DrawFn<T> for U where U: Fn(&T, &mut DisplayExclusive) -> Option<Rectangle> {}

pub trait IndicatorFormatFn<T>: Fn(&T) -> IndicatorState {}
impl<T, U> IndicatorFormatFn<T> for U where U: Fn(&T) -> IndicatorState {}

pub trait TextFormatFn<T>: Fn(&T) -> String {}
impl<T, U> TextFormatFn<T> for U where U: Fn(&T) -> String {}

pub trait FractionFormatFn<T>: Fn(&T) -> f32 {}
impl<T, U> FractionFormatFn<T> for U where U: Fn(&T) -> f32 {}

pub struct DynamicWidget<T: Sync + Send + 'static> {
    subscription_handle: SubscriptionHandle<T, Native>,
    join_handle: JoinHandle<Arc<Display>>,
}

impl<T: Serialize + DeserializeOwned + Send + Sync + Clone + 'static> DynamicWidget<T> {
    /// Create a generic dynamic widget
    ///
    /// # Arguments:
    ///
    /// * `topic`: The topic to subscribe to. If any change is detected on this
    ///   topic the area occupied by this widget is cleared and then redrawn.
    /// * `target`: The framebuffer to draw the widget on
    /// * `anchor`: A point passed through to the `draw_fn` that should somehow
    ///   correspond to the position the `draw_fn` draws to.
    ///   (This does however not have to be the case).
    /// * `draw_fn`: A function that is called whenever the widget should be
    ///   redrawn. The `draw_fn` should return a rectangle corresponding to the
    ///   bounding box it has drawn to.
    ///   The widget system takes care of clearing this area before redrawing.
    pub fn new(
        topic: Arc<Topic<T>>,
        display: Arc<Display>,
        draw_fn: Box<dyn DrawFn<T> + Sync + Send>,
    ) -> Self {
        let (mut rx, subscription_handle) = topic.subscribe_unbounded();

        let join_handle = spawn(async move {
            let mut prev_bb: Option<Rectangle> = None;

            while let Some(val) = rx.next().await {
                display.with_lock(|target| {
                    if let Some(bb) = prev_bb.take() {
                        // Clear the bounding box by painting it black
                        bb.into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                            .draw(&mut *target)
                            .unwrap();
                    }

                    prev_bb = draw_fn(&val, &mut *target);
                });
            }

            display
        });

        Self {
            subscription_handle,
            join_handle,
        }
    }

    /// Draw a self-updating status bar with a given `width` and `height`
    ///
    /// The `format_fn` should return a value between 0.0 and 1.0 indicating
    /// the fraction of the graph to fill.
    pub fn bar(
        topic: Arc<Topic<T>>,
        display: Arc<Display>,
        anchor: Point,
        width: u32,
        height: u32,
        format_fn: Box<dyn FractionFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::new(
            topic,
            display,
            Box::new(move |msg, target| {
                let val = format_fn(msg).clamp(0.0, 1.0);
                let fill_width = ((width as f32) * val) as u32;

                let bounding = Rectangle::new(anchor, Size::new(width, height));
                let filled = Rectangle::new(anchor, Size::new(fill_width, height));

                bounding
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                    .draw(target)
                    .unwrap();

                filled
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                    .draw(target)
                    .unwrap();

                Some(bounding)
            }),
        )
    }

    /// Draw an indicator bubble in an "On", "Off" or "Error" state
    pub fn indicator(
        topic: Arc<Topic<T>>,
        display: Arc<Display>,
        anchor: Point,
        format_fn: Box<dyn IndicatorFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::new(
            topic,
            display,
            Box::new(move |msg, target| {
                let ui_text_style: MonoTextStyle<BinaryColor> =
                    MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                match format_fn(msg) {
                    IndicatorState::On => {
                        let style = PrimitiveStyleBuilder::new()
                            .stroke_color(BinaryColor::On)
                            .stroke_width(2)
                            .fill_color(BinaryColor::On)
                            .build();
                        let circle = Circle::new(anchor, 10).into_styled(style);

                        circle.draw(target).unwrap();

                        Some(circle.bounding_box())
                    }
                    IndicatorState::Off => {
                        let circle = Circle::new(anchor, 10);

                        circle
                            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
                            .draw(target)
                            .unwrap();

                        Some(circle.bounding_box())
                    }
                    IndicatorState::Error => {
                        let text = Text::with_alignment(
                            "!",
                            anchor + Point::new(4, 10),
                            ui_text_style,
                            Alignment::Center,
                        );

                        text.draw(target).unwrap();

                        Some(text.bounding_box())
                    }
                    IndicatorState::Unkown => {
                        let text = Text::with_alignment(
                            "?",
                            anchor + Point::new(4, 10),
                            ui_text_style,
                            Alignment::Center,
                        );

                        text.draw(target).unwrap();

                        Some(text.bounding_box())
                    }
                }
            }),
        )
    }

    /// Draw self-updating text with configurable alignment
    pub fn text_aligned(
        topic: Arc<Topic<T>>,
        display: Arc<Display>,
        anchor: Point,
        format_fn: Box<dyn TextFormatFn<T> + Sync + Send>,
        alignment: Alignment,
    ) -> Self {
        Self::new(
            topic,
            display,
            Box::new(move |msg, target| {
                let text = format_fn(msg);

                let ui_text_style: MonoTextStyle<BinaryColor> =
                    MonoTextStyle::new(&UI_TEXT_FONT, BinaryColor::On);

                if !text.is_empty() {
                    let text = Text::with_alignment(&text, anchor, ui_text_style, alignment);
                    text.draw(target).unwrap();
                    Some(text.bounding_box())
                } else {
                    None
                }
            }),
        )
    }

    /// Draw self-updating left aligned text
    pub fn text(
        topic: Arc<Topic<T>>,
        display: Arc<Display>,
        anchor: Point,
        format_fn: Box<dyn TextFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::text_aligned(topic, display, anchor, format_fn, Alignment::Left)
    }

    /// Draw self-updating centered text
    pub fn text_center(
        topic: Arc<Topic<T>>,
        display: Arc<Display>,
        anchor: Point,
        format_fn: Box<dyn TextFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::text_aligned(topic, display, anchor, format_fn, Alignment::Center)
    }
}

impl DynamicWidget<i32> {
    /// Draw an animated locator widget at the side of the screen
    /// (if the locator is active).
    pub fn locator(topic: Arc<Topic<i32>>, display: Arc<Display>) -> Self {
        Self::new(
            topic,
            display,
            Box::new(move |val, target| {
                let size = 128 - (*val - 32).abs() * 4;

                if size != 0 {
                    let bounding = Rectangle::with_center(
                        Point::new(240 - 5, 120),
                        Size::new(10, size as u32),
                    );

                    bounding
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                        .draw(&mut *target)
                        .unwrap();

                    Some(bounding)
                } else {
                    None
                }
            }),
        )
    }
}

#[async_trait]
pub trait AnyWidget: Send + Sync {
    async fn unmount(self: Box<Self>) -> Arc<Display>;
}

#[async_trait]
impl<T: Sync + Send + Serialize + DeserializeOwned + 'static> AnyWidget for DynamicWidget<T> {
    /// Remove the widget from screen
    ///
    /// This has to be async, which is why it can not be performed by
    /// implementing the Drop trait.
    async fn unmount(mut self: Box<Self>) -> Arc<Display> {
        self.subscription_handle.unsubscribe();
        self.join_handle.await
    }
}
