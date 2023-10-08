pub use ratatui::widgets::ListState;

// FIXME: Move marking stuff here, to be able to use it in queue as well
pub trait StatefulList {
    fn get_size(&self) -> usize;
    fn select(&mut self, idx: Option<usize>);
    fn selected(&self) -> Option<usize>;

    fn first(&mut self) {
        if self.is_empty() {
            return;
        }
        self.select(Some(0));
    }

    fn last(&mut self) {
        if self.is_empty() {
            return;
        }
        self.select(Some(self.get_size() - 1));
    }

    fn next(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let next = if i == self.get_size() - 1 { 0 } else { i + 1 };
            self.select(Some(next));
        } else {
            self.select(Some(0));
        }
    }

    fn prev(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let prev = if i == 0 { self.get_size() - 1 } else { i - 1 };
            self.select(Some(prev));
        } else {
            self.select(Some(0));
        }
    }

    fn down(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let next = if i < self.get_size().saturating_sub(15) {
                i + 15
            } else {
                self.get_size() - 1
            };
            self.select(Some(next));
        } else {
            self.select(Some(0));
        }
    }

    fn up(&mut self) {
        if self.is_empty() {
            return;
        }
        if let Some(i) = self.selected() {
            let prev = if i < 15 { 0 } else { i.saturating_sub(15) };
            self.select(Some(prev));
        } else {
            self.select(Some(0));
        }
    }

    fn is_selected(&self) -> bool {
        self.selected().is_some()
    }

    fn is_empty(&self) -> bool {
        self.get_size() == 0
    }

    fn update_selection(&mut self) {
        if self.is_empty() {
            self.select(None);
            return;
        }
        match self.selected() {
            None => {
                self.select(Some(0));
            }
            Some(selected) => {
                if selected > self.get_size().saturating_sub(1) {
                    self.select(Some(self.get_size() - 1));
                }
            }
        }
    }
}
