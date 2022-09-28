use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task::{spawn, JoinHandle};
use async_trait::async_trait;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X9, MonoFont, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Text},
};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::FramebufferDrawTarget;
use crate::broker::{Native, SubscriptionHandle, Topic};

pub const UI_TEXT_FONT: MonoFont = FONT_6X9; // FIXME: Use font 6x8?

pub enum IndicatorState {
    On,
    Off,
    Error,
}

pub trait DrawFn<T>: Fn(&T, Point, &mut FramebufferDrawTarget) -> Option<Rectangle> {}
impl<T, U> DrawFn<T> for U where U: Fn(&T, Point, &mut FramebufferDrawTarget) -> Option<Rectangle> {}

pub trait IndicatorFormatFn<T>: Fn(&T) -> IndicatorState {}
impl<T, U> IndicatorFormatFn<T> for U where U: Fn(&T) -> IndicatorState {}

pub trait TextFormatFn<T>: Fn(&T) -> String {}
impl<T, U> TextFormatFn<T> for U where U: Fn(&T) -> String {}

pub trait FractionFormatFn<T>: Fn(&T) -> f32 {}
impl<T, U> FractionFormatFn<T> for U where U: Fn(&T) -> f32 {}

pub struct DynamicWidget<T: Sync + Send + 'static> {
    handles: Option<(SubscriptionHandle<T, Native>, JoinHandle<()>)>,
}

impl<T: Serialize + DeserializeOwned + Send + Sync + 'static> DynamicWidget<T> {
    /// Create a generic dynamic widget
    ///
    /// # Arguments:
    ///
    /// * `topic`: The topic to subscribe to. If any change is detected on this
    ///   topic the area occupied by this widget is cleared and then redrawed.
    /// * `target`: The framebuffer to draw the widget on
    /// * `anchor`: A point passed through to the `draw_fn` that should somehow
    ///   correspond to the position the `draw_fn` draws to.
    ///   (This does however not have to be the case).
    /// * `draw_fn`: A function that is called whenever the widget should be
    ///   redrawn. The `draw_fn` should return a rectangle corresponding to the
    ///   bounding box it has drawn to.
    ///   The widget system takes care of clearing this area before redrawing.
    pub async fn new(
        topic: Arc<Topic<T>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
        anchor: Point,
        draw_fn: Box<dyn DrawFn<T> + Sync + Send>,
    ) -> Self {
        let (mut rx, sub_handle) = topic.subscribe_unbounded().await;

        let join_handle = spawn(async move {
            let mut next = rx.next().await;

            while let Some(val) = next {
                let mut prev_bb = draw_fn(&val, anchor, &mut *target.lock().await);

                next = rx.next().await;

                if let Some(bb) = prev_bb.take() {
                    // Clear the bounding box by painting it black
                    bb.into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                        .draw(&mut *target.lock().await)
                        .unwrap();
                }
            }
        });

        Self {
            handles: Some((sub_handle, join_handle)),
        }
    }

    /// Draw a self-updating status bar with a given `width` and `height`
    ///
    /// The `format_fn` should return a value between 0.0 and 1.0 indicating
    /// the fraction of the graph to fill.
    pub async fn bar(
        topic: Arc<Topic<T>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
        anchor: Point,
        width: u32,
        height: u32,
        format_fn: Box<dyn FractionFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::new(
            topic,
            target,
            anchor,
            Box::new(move |msg, anchor, target| {
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
        .await
    }

    /// Draw an indicator bubble in an "On", "Off" or "Error" state
    pub async fn indicator(
        topic: Arc<Topic<T>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
        anchor: Point,
        format_fn: Box<dyn IndicatorFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::new(
            topic,
            target,
            anchor,
            Box::new(move |msg, anchor, target| match format_fn(msg) {
                IndicatorState::On => {
                    let circle = Circle::new(anchor, 10);
                    let style = PrimitiveStyleBuilder::new()
                        .stroke_color(BinaryColor::On)
                        .stroke_width(2)
                        .fill_color(BinaryColor::On)
                        .build();

                    circle.into_styled(style).draw(target).unwrap();

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
                    let lines = [
                        Line::new(Point::new(0, 0), Point::new(10, 10)).translate(anchor),
                        Line::new(Point::new(0, 10), Point::new(10, 0)).translate(anchor),
                    ];

                    for line in &lines {
                        line.into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
                            .draw(target)
                            .unwrap();
                    }

                    Some(lines[0].bounding_box())
                }
            }),
        )
        .await
    }

    /// Draw self-updating text with configurable alignment
    pub async fn text_aligned(
        topic: Arc<Topic<T>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
        anchor: Point,
        format_fn: Box<dyn TextFormatFn<T> + Sync + Send>,
        alignment: Alignment,
    ) -> Self {
        Self::new(
            topic,
            target,
            anchor,
            Box::new(move |msg, anchor, target| {
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
        .await
    }

    /// Draw self-updating left aligned text
    pub async fn text(
        topic: Arc<Topic<T>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
        anchor: Point,
        format_fn: Box<dyn TextFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::text_aligned(topic, target, anchor, format_fn, Alignment::Left).await
    }

    /// Draw self-updating centered text
    pub async fn text_center(
        topic: Arc<Topic<T>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
        anchor: Point,
        format_fn: Box<dyn TextFormatFn<T> + Sync + Send>,
    ) -> Self {
        Self::text_aligned(topic, target, anchor, format_fn, Alignment::Center).await
    }
}

impl DynamicWidget<i32> {
    /// Draw an animated locator widget at the side of the screen
    /// (if the locator is active).
    pub async fn locator(
        topic: Arc<Topic<i32>>,
        target: Arc<Mutex<FramebufferDrawTarget>>,
    ) -> Self {
        Self::new(
            topic,
            target,
            Point::new(128 - 5, 32),
            Box::new(move |val, anchor, target| {
                let size = 64 - ((*val - 32).abs() * 2);

                if size != 0 {
                    let bounding = Rectangle::with_center(anchor, Size::new(10, size as u32));

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
        .await
    }
}

#[async_trait]
pub trait AnyWidget: Send + Sync {
    async fn unmount(&mut self);
}

#[async_trait]
impl<T: Sync + Send + Serialize + DeserializeOwned + 'static> AnyWidget for DynamicWidget<T> {
    /// Remove the widget from screen
    ///
    /// This has to be async, which is why it can not be performed by
    /// implementing the Drop trait.
    async fn unmount(&mut self) {
        if let Some((sh, jh)) = self.handles.take() {
            sh.unsubscribe().await;
            jh.await;
        }
    }
}
