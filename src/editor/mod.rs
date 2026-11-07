pub mod buffer;

use anyhow::{Context, Result};
pub use buffer::TextBuffer;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Editor {
    pub buffer: TextBuffer,
    pub file_path: Option<PathBuf>,
    #[allow(dead_code)]
    pub title: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
            file_path: None,
            title: "Untitled".to_string(),
        }
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file from {:?}", path))?;

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let title = crate::workspace::Workspace::extract_title(&content, &stem);

        Ok(Self {
            buffer: TextBuffer::from_str(&content),
            file_path: Some(path.to_path_buf()),
            title,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        if let Some(path) = &self.file_path {
            let text = self.buffer.get_text();
            fs::write(path, text)
                .with_context(|| format!("Failed to save file to {:?}", path))?;
            self.buffer.is_dirty = false;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        let text = self.buffer.get_text();
        fs::write(path, text)
            .with_context(|| format!("Failed to save file to {:?}", path))?;
        self.file_path = Some(path.to_path_buf());
        self.buffer.is_dirty = false;
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.is_dirty
    }

    #[allow(dead_code)]
    pub fn cursor_pos(&self) -> (usize, usize) {
        (self.buffer.cursor_line, self.buffer.cursor_col)
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }
}
