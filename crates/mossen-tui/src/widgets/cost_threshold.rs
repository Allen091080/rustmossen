use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::render_glyphs::RenderGlyphs;
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct CostThresholdDialogState {
    pub current_cost: f64,
    pub threshold: f64,
    pub acknowledged: bool,
}

impl CostThresholdDialogState {
    pub fn new(cost: f64, threshold: f64) -> Self {
        Self {
            current_cost: cost,
            threshold,
            acknowledged: false,
        }
    }
}

pub struct CostThresholdDialogWidget<'a> {
    state: &'a CostThresholdDialogState,
    theme: &'a Theme,
    glyphs: RenderGlyphs,
}

impl<'a> CostThresholdDialogWidget<'a> {
    pub fn new(state: &'a CostThresholdDialogState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            glyphs: RenderGlyphs::default(),
        }
    }

    pub fn glyphs(mut self, glyphs: RenderGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }
}

impl<'a> Widget for CostThresholdDialogWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 24 || area.height < 5 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(self.glyphs.border)
            .border_style(self.theme.style_border())
            .title(Span::styled(
                " Cost Threshold Reached ",
                Style::default()
                    .fg(self.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let lines = vec![
            Line::from(vec![
                Span::styled("Current cost: ", Style::default().fg(self.theme.text)),
                Span::styled(
                    format!("${:.2}", self.state.current_cost),
                    Style::default()
                        .fg(self.theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Threshold: ", Style::default().fg(self.theme.text)),
                Span::styled(
                    format!("${:.2}", self.state.threshold),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                format!("Enter continue{}Esc stop", self.glyphs.separator()),
                Style::default().fg(self.theme.text_dim),
            )),
        ];
        Paragraph::new(lines).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn cost_threshold_dialog_renders_without_legacy_dialog() {
        let state = CostThresholdDialogState::new(12.34, 10.0);
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 52, 8));

        CostThresholdDialogWidget::new(&state, &theme).render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains("Cost Threshold Reached"));
        assert!(rendered.contains("$12.34"));
        assert!(rendered.contains("$10.00"));
    }

    #[test]
    fn cost_threshold_dialog_can_render_ascii_chrome() {
        let state = CostThresholdDialogState::new(12.34, 10.0);
        let theme = Theme::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 52, 8));

        CostThresholdDialogWidget::new(&state, &theme)
            .glyphs(RenderGlyphs::ascii())
            .render(buf.area, &mut buf);

        let rendered = buffer_text(&buf);
        assert!(rendered.contains('+'), "{rendered}");
        assert!(rendered.contains("Enter continue - Esc stop"), "{rendered}");
        assert!(!rendered.contains('·'), "{rendered}");
    }
}
