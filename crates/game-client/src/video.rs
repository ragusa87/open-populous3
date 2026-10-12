//! A headless video (`VIDEO=secs`, `dev::Capture::Video`): each captured frame's pixels are piped raw into an
//! `ffmpeg` child encoding `VIDEO_PATH`, no image written per frame. Screenshots are read back asynchronously, so
//! frames are put back in order (`InOrder`) before they are written.

use bevy::prelude::*;
use bevy::render::view::screenshot::ScreenshotCaptured;
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

/// Frames per second of a video.
pub const FPS: u32 = 30;
/// Where the video goes, overwritten each time (git-ignored).
pub const VIDEO_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/video.mp4");

/// Frames numbered from 1, arriving in any order, handed out in order.
#[derive(Default)]
pub struct InOrder {
    next: u32,
    waiting: BTreeMap<u32, Vec<u8>>,
}

impl InOrder {
    /// Takes frame `n`; the frames now ready, in order.
    pub fn push(&mut self, n: u32, frame: Vec<u8>) -> Vec<Vec<u8>> {
        self.next = self.next.max(1);
        self.waiting.insert(n, frame);
        let mut ready = Vec::new();
        while let Some(frame) = self.waiting.remove(&self.next) {
            ready.push(frame);
            self.next += 1;
        }
        ready
    }
}

/// The encoder, started at the first frame (its size is the frame's).
#[derive(Resource, Default)]
pub struct Video {
    order: InOrder,
    ffmpeg: Option<(Child, ChildStdin)>,
    failed: bool,
}

impl Video {
    fn start(width: u32, height: u32) -> std::io::Result<(Child, ChildStdin)> {
        let mut child = Command::new("ffmpeg")
            .args(["-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{width}x{height}"), "-r", &FPS.to_string(), "-i", "-"])
            .args(["-pix_fmt", "yuv420p", VIDEO_PATH])
            .stdin(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| std::io::Error::other("no stdin"))?;
        Ok((child, stdin))
    }

    /// Frame `n` (from 1) captured: written once the ones before it are.
    fn add(&mut self, n: u32, image: &Image) {
        if self.failed {
            return;
        }
        if self.ffmpeg.is_none() {
            match Video::start(image.width(), image.height()) {
                Ok(f) => self.ffmpeg = Some(f),
                Err(e) => {
                    error!("video: cannot start ffmpeg: {e}");
                    self.failed = true;
                    return;
                }
            }
        }
        let Some((_, stdin)) = self.ffmpeg.as_mut() else { return };
        for frame in self.order.push(n, image.data.clone().unwrap_or_default()) {
            if let Err(e) = stdin.write_all(&frame) {
                error!("video: {e}");
                self.failed = true;
                return;
            }
        }
    }

    /// Ends the stream and waits for ffmpeg to write the file.
    pub fn finish(&mut self) {
        if let Some((mut child, stdin)) = self.ffmpeg.take() {
            drop(stdin);
            match child.wait() {
                Ok(status) if status.success() => info!("video saved to {VIDEO_PATH}"),
                Ok(status) => error!("video: ffmpeg {status}"),
                Err(e) => error!("video: {e}"),
            }
        }
    }
}

/// The observer of a screenshot that is frame `n` of the video.
pub fn to_video(n: u32) -> impl FnMut(On<ScreenshotCaptured>, ResMut<Video>) {
    move |captured, mut video| video.add(n, &captured.image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_come_out_in_order_whatever_order_they_arrive_in() {
        let mut o = InOrder::default();
        assert!(o.push(2, vec![2]).is_empty(), "waits for 1");
        assert_eq!(o.push(1, vec![1]), [vec![1], vec![2]]);
        assert!(o.push(4, vec![4]).is_empty());
        assert_eq!(o.push(3, vec![3]), [vec![3], vec![4]]);
        assert_eq!(o.push(5, vec![5]), [vec![5]]);
    }
}
