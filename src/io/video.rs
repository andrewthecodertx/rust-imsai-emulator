use std::io::{self, Write};

/// Video display controller for the IMSAI 8080
pub struct VideoDisplay {
    cursor_x: usize,
    cursor_y: usize,
    width: usize,
    height: usize,
    buffer: Vec<Vec<char>>,
    pub auto_render: bool,
}

impl VideoDisplay {
    pub fn new(width: usize, height: usize) -> Self {
        let mut buffer = Vec::with_capacity(height);
        for _ in 0..height {
            buffer.push(vec![' '; width]);
        }

        Self {
            cursor_x: 0,
            cursor_y: 0,
            width,
            height,
            buffer,
            auto_render: true,
        }
    }

    pub fn write_char(&mut self, ch: u8) {
        let ch = ch as char;

        match ch {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= self.height {
                    self.scroll_up();
                    self.cursor_y = self.height - 1;
                }
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\x08' => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.buffer[self.cursor_y][self.cursor_x] = ' ';
                }
            }
            _ => {
                if self.cursor_x < self.width && self.cursor_y < self.height {
                    self.buffer[self.cursor_y][self.cursor_x] = ch;
                    self.cursor_x += 1;

                    if self.cursor_x >= self.width {
                        self.cursor_x = 0;
                        self.cursor_y += 1;

                        if self.cursor_y >= self.height {
                            self.scroll_up();
                            self.cursor_y = self.height - 1;
                        }
                    }
                }
            }
        }
    }

    fn scroll_up(&mut self) {
        for y in 0..self.height - 1 {
            for x in 0..self.width {
                self.buffer[y][x] = self.buffer[y + 1][x];
            }
        }

        for x in 0..self.width {
            self.buffer[self.height - 1][x] = ' ';
        }
    }

    pub fn clear(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.buffer[y][x] = ' ';
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    pub fn render(&self) {
        if !self.auto_render {
            return;
        }

        print!("\x1B[H");

        for y in 0..self.height {
            for x in 0..self.width {
                print!("{}", self.buffer[y][x]);
            }
            println!();
        }

        let _ = io::stdout().flush();
    }

    pub fn get_display_string(&self) -> String {
        let mut result = String::new();
        for y in 0..self.height {
            for x in 0..self.width {
                result.push(self.buffer[y][x]);
            }
            result.push('\n');
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_display_basic() {
        let mut display = VideoDisplay::new(10, 5);

        display.write_char(b'H');
        display.write_char(b'e');
        display.write_char(b'l');
        display.write_char(b'l');
        display.write_char(b'o');

        let output = display.get_display_string();
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_video_display_newline() {
        let mut display = VideoDisplay::new(10, 5);

        display.write_char(b'H');
        display.write_char(b'i');
        display.write_char(b'\n');
        display.write_char(b'B');
        display.write_char(b'y');
        display.write_char(b'e');

        let output = display.get_display_string();
        assert!(output.starts_with("Hi"));
        assert!(output.contains("Bye"));
    }
}
