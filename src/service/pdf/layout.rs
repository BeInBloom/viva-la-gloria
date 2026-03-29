#[derive(Debug, Clone, Copy)]
pub(super) struct SizeMm {
    width: f64,
    height: f64,
}

impl SizeMm {
    const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub(super) fn width(self) -> f64 {
        self.width
    }

    pub(super) fn height(self) -> f64 {
        self.height
    }
}

pub(super) const PAGE_SIZE_MM: SizeMm = SizeMm::new(210.0, 297.0);
pub(super) const CARD_SIZE_MM: SizeMm = SizeMm::new(63.0, 88.0);

#[derive(Debug, Clone, Copy)]
pub(super) struct Layout {
    cards_per_row: usize,
    cards_per_page: usize,
    margin_left_mm: f64,
    margin_top_mm: f64,
}

impl Layout {
    pub(super) fn new(page: SizeMm, card: SizeMm) -> Self {
        let cards_per_row = (page.width / card.width).floor() as usize;
        let cards_per_column = (page.height / card.height).floor() as usize;
        let used_width = cards_per_row as f64 * card.width;
        let used_height = cards_per_column as f64 * card.height;

        Self {
            cards_per_row,
            cards_per_page: cards_per_row * cards_per_column,
            margin_left_mm: (page.width - used_width) / 2.0,
            margin_top_mm: (page.height - used_height) / 2.0,
        }
    }

    pub(super) fn cards_per_page(&self) -> usize {
        self.cards_per_page
    }

    pub(super) fn position_for_slot(&self, slot: usize) -> (f64, f64) {
        let row = slot / self.cards_per_row;
        let column = slot % self.cards_per_row;

        (
            self.margin_left_mm + column as f64 * CARD_SIZE_MM.width,
            self.margin_top_mm + row as f64 * CARD_SIZE_MM.height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{CARD_SIZE_MM, Layout, PAGE_SIZE_MM};

    #[test]
    fn layout_for_a4_cards_uses_expected_grid_and_margins() {
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        assert_eq!(layout.cards_per_row, 3);
        assert_eq!(layout.cards_per_page, 9);
        assert_eq!(layout.margin_left_mm, 10.5);
        assert_eq!(layout.margin_top_mm, 16.5);
    }

    #[test]
    fn layout_positions_center_card_grid_slots() {
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        assert_eq!(layout.position_for_slot(0), (10.5, 16.5));
        assert_eq!(layout.position_for_slot(4), (73.5, 104.5));
        assert_eq!(layout.position_for_slot(8), (136.5, 192.5));
    }
}
