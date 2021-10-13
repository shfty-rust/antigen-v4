use std::any::TypeId;

use reflection::data::Data;
use tui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};

use crate::{DataWidget, ReflectionWidgetState, WidgetState};

pub struct FloatWidget<'a, T: ToString>(&'a mut T);

impl<'a, T: ToString> From<&'a mut T> for FloatWidget<'a, T> {
    fn from(v: &'a mut T) -> Self {
        FloatWidget(v)
    }
}

impl<'a, T: ToString> WidgetState for FloatWidget<'a, T> {}

impl<'a, T: ToString> DataWidget for FloatWidget<'a, T> {
    fn size_complex(
        &mut self,
        area: Rect,
        _predicate: &dyn Fn(&mut Data, TypeId) -> Option<Box<dyn DataWidget + '_>>,
    ) -> (u16, u16) {
        (
            (self.0.to_string().len() as u16).min(area.width),
            1.min(area.height),
        )
    }

    fn render_complex_impl(
        &mut self,
        mut layout: crate::LayoutIterator,
        buf: &mut Buffer,
        _state: &mut ReflectionWidgetState,
        _predicate: &dyn Fn(&mut Data, TypeId) -> Option<Box<dyn DataWidget + '_>>,
    ) {
        if let Some(area) = layout
            .next()
            .expect("Insufficient layout cells for widget list")
        {
            Paragraph::new(self.0.to_string()).render(area, buf)
        }
    }
}
