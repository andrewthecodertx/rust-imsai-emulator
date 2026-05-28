//! Video display interface for the IMSAI 8080 emulator
//!
//! This module provides a simulated video display interface that can be used
//! to display output from the emulated system. It implements a simple
//! text-based terminal interface similar to what would be found in real
//! S-100 systems like the SD Systems 8024 video board.

use std::io::{self, Write};

/// Video display controller for the IMSAI 8080
pub struct VideoDisplay {
    /// Current cursor position (row, column)
    cursor_x: usize,
    cursor_y: usize,
    /// Width of the display in characters
    width: usize,
    /// Height of the display in characters
    height: usize,
    /// Text buffer for the display
    buffer: Vec<Vec<char>>,
    /// Whether to render to stdout on every write (disabled in terminal mode)
    pub auto_render: bool,
}

impl VideoDisplay {
    /// Create a new video display controller
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

    /// Write a character to the display at the current cursor position
    pub fn write_char(&mut self, ch: u8) {
        let ch = ch as char;

        match ch {
            '\n' => {
                // Move to beginning of next line
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= self.height {
                    self.scroll_up();
                    self.cursor_y = self.height - 1;
                }
            }
            '\r' => {
                // Move to beginning of current line
                self.cursor_x = 0;
            }
            '\x08' => {
                // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.buffer[self.cursor_y][self.cursor_x] = ' ';
                }
            }
            _ => {
                // Regular character
                if self.cursor_x < self.width && self.cursor_y < self.height {
                    self.buffer[self.cursor_y][self.cursor_x] = ch;
                    self.cursor_x += 1;

                    // Wrap to next line if needed
                    if self.cursor_x >= self.width {
                        self.cursor_x = 0;
                        self.cursor_y += 1;

                        // Scroll if we've reached the bottom
                        if self.cursor_y >= self.height {
                            self.scroll_up();
                            self.cursor_y = self.height - 1;
                        }
                    }
                }
            }
        }
    }

    /// Scroll the display up by one line
    fn scroll_up(&mut self) {
        // Move all lines up by one
        for y in 0..self.height - 1 {
            for x in 0..self.width {
                self.buffer[y][x] = self.buffer[y + 1][x];
            }
        }

        // Clear the bottom line
        for x in 0..self.width {
            self.buffer[self.height - 1][x] = ' ';
        }
    }

    /// Clear the display
    pub fn clear(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.buffer[y][x] = ' ';
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Render the display to stdout
    pub fn render(&self) {
        if !self.auto_render {
            return;
        }
        // Move cursor to top-left of terminal
        print!("\x1B[H");

        // Print each line of the buffer
        for y in 0..self.height {
            for x in 0..self.width {
                print!("{}", self.buffer[y][x]);
            }
            println!();
        }

        // Flush output
        io::stdout().flush().unwrap();
    }

    /// Get the current display as a string
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
