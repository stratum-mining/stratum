use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use async_std::{prelude::*, task};
use pin_project_lite::pin_project;

pub fn is_outdated(start_timestamp_secs: u64, max_delta: u32) -> bool {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();
    (now_secs - start_timestamp_secs) as u32 >= max_delta
}

pin_project! {
    /// Based on `async_std::io::Lines` but with a limit on the number of bytes per line.
    #[derive(Debug)]
    pub struct LimitedLines<R> {
        #[pin]
        pub(crate) reader: R,
        pub(crate) buf: String,
        pub(crate) bytes: Vec<u8>,
        pub(crate) read: usize,
        pub(crate) limit: usize,
    }
}

impl<R: futures::AsyncBufRead> LimitedLines<R> {
    pub(crate) fn new(reader: R, limit: usize) -> Self {
        Self {
            reader,
            buf: String::new(),
            bytes: Vec::new(),
            read: 0,
            limit,
        }
    }
}

impl<R: futures::AsyncBufRead> Stream for LimitedLines<R> {
    type Item = io::Result<String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let n = match read_line_internal_limited(
            this.reader,
            cx,
            this.buf,
            this.bytes,
            this.read,
            *this.limit,
        ) {
            task::Poll::Ready(t) => t,
            task::Poll::Pending => return task::Poll::Pending,
        }?;
        if n == 0 && this.buf.is_empty() {
            return Poll::Ready(None);
        }
        if this.buf.ends_with('\n') {
            this.buf.pop();
            if this.buf.ends_with('\r') {
                this.buf.pop();
            }
        }
        Poll::Ready(Some(Ok(std::mem::take(this.buf))))
    }
}

fn read_line_internal_limited<R: futures::AsyncBufRead + ?Sized>(
    reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut String,
    bytes: &mut Vec<u8>,
    read: &mut usize,
    limit: usize,
) -> Poll<io::Result<usize>> {
    let ret = match read_until_internal_limited(reader, cx, b'\n', bytes, read, limit) {
        task::Poll::Ready(t) => t,
        task::Poll::Pending => return task::Poll::Pending,
    };
    if std::str::from_utf8(bytes).is_err() {
        Poll::Ready(ret.and_then(|_| {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            ))
        }))
    } else {
        debug_assert!(buf.is_empty());
        debug_assert_eq!(*read, 0);
        // Safety: `bytes` is a valid UTF-8 because `str::from_utf8` returned `Ok`.
        std::mem::swap(unsafe { buf.as_mut_vec() }, bytes);
        Poll::Ready(ret)
    }
}

fn read_until_internal_limited<R: async_std::io::prelude::BufReadExt + ?Sized>(
    mut reader: std::pin::Pin<&mut R>,
    cx: &mut task::Context<'_>,
    byte: u8,
    buf: &mut Vec<u8>,
    read: &mut usize,
    limit: usize,
) -> task::Poll<std::io::Result<usize>> {
    loop {
        let (done, used) = {
            let available = match reader.as_mut().poll_fill_buf(cx) {
                task::Poll::Ready(t) => t,
                task::Poll::Pending => return task::Poll::Pending,
            }?;
            if let Some(i) = available.iter().position(|&b| b == byte) {
                if buf.len() + i > limit {
                    return task::Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "line is too long",
                    )));
                }
                buf.extend_from_slice(&available[..=i]);
                (true, i + 1)
            } else if buf.len() + available.len() > limit {
                return task::Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "line is too long",
                )));
            } else {
                buf.extend_from_slice(available);
                (false, available.len())
            }
        };
        reader.as_mut().consume(used);
        *read += used;
        if done || used == 0 {
            return task::Poll::Ready(Ok(std::mem::replace(read, 0)));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_line_buf_reader_limited() {
        use super::*;
        use async_std::io::BufReader;
        async_std::task::block_on(async {
            async fn compare_bufreaders(input: &str) {
                let limit = input.len();
                let reader = BufReader::new(input.as_bytes());
                let mut limited_lines = LimitedLines::new(reader, limit);
                let mut unlimited_lines = BufReader::new(input.as_bytes()).lines();
                loop {
                    if let Some(Ok(unlimited_line)) = unlimited_lines.next().await {
                        let limited_line = limited_lines.next().await.unwrap().unwrap();
                        assert_eq!(unlimited_line, limited_line);
                    } else {
                        assert!(limited_lines.next().await.is_none());
                        break;
                    }
                }
            }
            async fn test_limited(input: &str, limit: usize, expected: Vec<&str>, fail_last: bool) {
                let reader = BufReader::new(input.as_bytes());
                let mut stream = LimitedLines::new(reader, limit);
                for e in expected {
                    let line = stream.next().await.unwrap().unwrap();
                    assert_eq!(line, e);
                }
                if fail_last {
                    assert_eq!(
                        stream.next().await.unwrap().unwrap_err().kind(),
                        std::io::ErrorKind::InvalidData
                    );
                } else {
                    assert!(stream.next().await.is_none());
                }
            }
            // Behavior should be the same as async_std::io::BufReader::lines
            compare_bufreaders("").await;
            compare_bufreaders("\n").await;
            compare_bufreaders("\n\n").await;
            compare_bufreaders("\n\n\n").await;
            compare_bufreaders("hello").await;
            compare_bufreaders("hello\n").await;
            compare_bufreaders("hello\nworld").await;
            compare_bufreaders("hello\nworld\n").await;
            compare_bufreaders("\r").await;
            compare_bufreaders("\r\n").await;
            compare_bufreaders("\r\n\n").await;
            compare_bufreaders("\r\n\r\n").await;
            compare_bufreaders("\n\r\n").await;
            compare_bufreaders("\n\r").await;
            compare_bufreaders("hello\r").await;
            compare_bufreaders("hello\r\n").await;
            compare_bufreaders("hello\r\nworld").await;
            compare_bufreaders("hello\r\nworld\r\n").await;
            // note: `\n` doesn't counts towards the limit, but `\r` does
            // these will not fail because the limit is not reached
            test_limited("\n", 0, vec![""], false).await;
            test_limited("\r\n", 1, vec![""], false).await;
            test_limited("\r\n\r\n", 1, vec!["", ""], false).await;
            test_limited("hello\n", 5, vec!["hello"], false).await;
            test_limited("hello", 5, vec!["hello"], false).await;
            test_limited(
                "hello\n\n\n\n\n\n\n\n",
                5,
                vec!["hello", "", "", "", "", "", "", ""],
                false,
            )
            .await;
            test_limited("hello\n\r\n", 5, vec!["hello", ""], false).await;
            test_limited("hello\nworld", 5, vec!["hello", "world"], false).await;
            test_limited("hello\nworld\n", 5, vec!["hello", "world"], false).await;
            // these will fail because the limit is reached
            test_limited("\r\n", 0, vec![], true).await;
            test_limited("hello\r\n", 5, vec![], true).await;
            test_limited("hello\n", 4, vec![], true).await;
            test_limited("hello\n\n", 4, vec![], true).await;
            test_limited("hello", 4, vec![], true).await;
            test_limited("hello\nworld\r\n", 5, vec!["hello"], true).await;
        });
    }
}
