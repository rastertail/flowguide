use std::future::Future;

use anyhow::{Error, Result};
use futures::future::LocalBoxFuture;

pub struct AsyncStreamReader {
    buf: Vec<u8>,
    last_end: usize,
    next_buffer: Box<dyn FnMut() -> LocalBoxFuture<'static, Option<Vec<u8>>>>,
}

impl AsyncStreamReader {
    pub fn new<F: Future<Output = Option<Vec<u8>>> + 'static, U: (FnMut() -> F) + 'static>(
        mut next_buffer: U,
    ) -> Self {
        Self {
            buf: Vec::new(),
            last_end: 0,
            next_buffer: Box::new(move || Box::pin(next_buffer())),
        }
    }

    fn shift_leftovers(&mut self) {
        self.buf = self.buf[self.last_end..].to_vec();
    }

    pub async fn read_line(&mut self) -> Result<&[u8]> {
        self.shift_leftovers();

        let mut len = 0;
        loop {
            if let Some(idx) = self.buf[len..].iter().position(|b| *b == b'\n') {
                len += idx + 1;
                break;
            }
            len = self.buf.len();
            let mut next = (self.next_buffer)()
                .await
                .ok_or_else(|| Error::msg("Reached EOF before a complete line"))?;
            self.buf.append(&mut next);
        }
        self.last_end = len;
        Ok(&self.buf[..len - 1])
    }

    pub async fn read_line_utf8(&mut self) -> Result<&str> {
        Ok(std::str::from_utf8(self.read_line().await?)?)
    }

    pub async fn read_exact(&mut self, len: usize) -> Result<&[u8]> {
        if self.buf.len() < self.last_end + len {
            self.shift_leftovers();
            self.last_end = 0;
        }

        while self.buf.len() < len {
            let mut next = (self.next_buffer)()
                .await
                .ok_or_else(|| Error::msg("Reached EOF before filling buffer"))?;
            self.buf.append(&mut next);
        }

        let start = self.last_end;
        self.last_end += len;
        return Ok(&self.buf[start..start + len]);
    }
}
