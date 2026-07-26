pub struct InputState {
    pub content: String,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.content[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.content.remove(prev);
        self.cursor = prev;
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.content[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    pub fn move_right(&mut self) {
        if self.cursor >= self.content.len() {
            return;
        }
        let next = self.content[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
            .unwrap_or(self.content.len());
        self.cursor = next;
    }

    pub fn cursor_x(&self) -> u16 {
        self.content[..self.cursor].chars().count() as u16
    }

    pub fn replace_input(&mut self, text: String) {
        self.content = text;
        self.cursor = self.content.len();
    }

    pub fn submit(&mut self) -> String {
        let content = self.content.clone();
        if !content.trim().is_empty() {
            self.history.push(content.clone());
        }
        self.content.clear();
        self.cursor = 0;
        self.history_index = None;
        content
    }

    pub fn scroll_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_index = match self.history_index {
            Some(i) if i > 0 => Some(i - 1),
            Some(i) => Some(i),
            None => Some(self.history.len() - 1),
        };
        if let Some(i) = new_index {
            self.history_index = Some(i);
            self.content = self.history[i].clone();
            self.cursor = self.content.len();
        }
    }

    pub fn scroll_down(&mut self) {
        if let Some(i) = self.history_index {
            if i + 1 < self.history.len() {
                self.history_index = Some(i + 1);
                self.content = self.history[i + 1].clone();
                self.cursor = self.content.len();
            } else {
                self.history_index = None;
                self.content.clear();
                self.cursor = 0;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}
