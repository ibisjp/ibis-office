// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 動画・音声の再生: a clip played by the system's player (AVFoundation's
//! `AVPlayer` on macOS). A video's frames are taken one by one from an
//! `AVPlayerItemVideoOutput` as the CoreVideo buffers GPUI draws as a
//! surface. Windows uses MediaPlayer with D3D11 frame-server surfaces.
//! Other platforms play nothing yet (`Player::open` is `None`).

#[cfg(target_os = "macos")]
pub(crate) use mac::{Frame, Player, video_aspect};
/// Let the system run its main loop for `seconds` (a test has no app loop).
#[cfg(all(target_os = "macos", feature = "scenarios"))]
pub(crate) use mac::run_loop;

#[cfg(not(any(target_os = "macos",windows)))]
pub(crate) use other::{Frame, Player, video_aspect};
#[cfg(windows)]
#[path="media_windows.rs"]
mod windows_player;
#[cfg(windows)]
pub(crate) use windows_player::{Frame,Player,video_aspect};

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod mac {
    use std::ffi::c_void;
    use std::path::Path;

    use core_foundation::base::TCFType;
    use core_video::pixel_buffer::{CVPixelBuffer, CVPixelBufferRef};
    use objc::runtime::{BOOL, Object};
    use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};

    type Id = *mut Object;

    /// A video frame, as GPUI draws it.
    pub(crate) type Frame = CVPixelBuffer;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CMTime {
        value: i64,
        timescale: i32,
        flags: u32,
        epoch: i64,
    }

    unsafe impl Encode for CMTime {
        fn encode() -> Encoding {
            unsafe { Encoding::from_str("{?=qiIq}") }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    unsafe impl Encode for CGSize {
        fn encode() -> Encoding {
            unsafe { Encoding::from_str("{CGSize=dd}") }
        }
    }

    #[link(name = "AVFoundation", kind = "framework")]
    #[link(name = "CoreMedia", kind = "framework")]
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {
        fn CMTimeGetSeconds(time: CMTime) -> f64;
        fn CACurrentMediaTime() -> f64;
    }

    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        static kCVPixelBufferPixelFormatTypeKey: Id;
    }

    /// `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`, the one GPUI draws.
    const FORMAT_420F: u32 = 0x3432_3066;
    const VALID: u32 = 1;

    fn string(text: &str) -> Id {
        unsafe {
            let s: Id = msg_send![class!(NSString), alloc];
            let s: Id = msg_send![s, initWithBytes: text.as_ptr() as *const c_void length: text.len() encoding: 4usize];
            msg_send![s, autorelease]
        }
    }

    fn url(path: &Path) -> Id {
        unsafe { msg_send![class!(NSURL), fileURLWithPath: string(&path.to_string_lossy())] }
    }

    /// The width over the height of the video in `path`, if it has a video track.
    pub(crate) fn video_aspect(path: &Path) -> Option<f64> {
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let asset: Id = msg_send![class!(AVURLAsset), URLAssetWithURL: url(path) options: std::ptr::null_mut::<Object>()];
            let tracks: Id = msg_send![asset, tracksWithMediaType: string("vide")];
            let count: usize = msg_send![tracks, count];
            let aspect = if count > 0 {
                let track: Id = msg_send![tracks, objectAtIndex: 0usize];
                let size: CGSize = msg_send![track, naturalSize];
                (size.width > 0.0 && size.height > 0.0).then(|| size.width / size.height)
            } else {
                None
            };
            let _: () = msg_send![pool, drain];
            aspect
        }
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
    }

    /// Let the system run its main loop for `seconds`.
    #[allow(dead_code)]
    pub(crate) fn run_loop(seconds: f64) {
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, 0);
        }
    }

    /// A clip being played.
    pub(crate) struct Player {
        player: Id,
        item: Id,
        output: Id,
    }

    impl Player {
        /// The clip in `path`, ready to play; `video` takes its frames too.
        pub(crate) fn open(path: &Path, video: bool) -> Option<Player> {
            unsafe {
                let pool: Id = msg_send![class!(NSAutoreleasePool), new];
                let item: Id = msg_send![class!(AVPlayerItem), playerItemWithURL: url(path)];
                if item.is_null() {
                    let _: () = msg_send![pool, drain];
                    return None;
                }
                let mut output: Id = std::ptr::null_mut();
                if video {
                    let format: Id = msg_send![class!(NSNumber), numberWithUnsignedInt: FORMAT_420F];
                    let attributes: Id = msg_send![class!(NSDictionary), dictionaryWithObject: format forKey: kCVPixelBufferPixelFormatTypeKey];
                    let o: Id = msg_send![class!(AVPlayerItemVideoOutput), alloc];
                    output = msg_send![o, initWithPixelBufferAttributes: attributes];
                    let _: () = msg_send![item, addOutput: output];
                }
                let player: Id = msg_send![class!(AVPlayer), playerWithPlayerItem: item];
                let _: Id = msg_send![player, retain];
                let _: Id = msg_send![item, retain];
                let _: () = msg_send![pool, drain];
                Some(Player { player, item, output })
            }
        }

        pub(crate) fn play(&self) {
            unsafe {
                let _: () = msg_send![self.player, play];
            }
        }

        pub(crate) fn pause(&self) {
            unsafe {
                let _: () = msg_send![self.player, pause];
            }
        }

        /// Paused and back at the start.
        pub(crate) fn rewind(&self) {
            unsafe {
                let _: () = msg_send![self.player, pause];
                let zero = CMTime { value: 0, timescale: 1, flags: VALID, epoch: 0 };
                let _: () = msg_send![self.player, seekToTime: zero];
            }
        }

        /// Seek to time in milliseconds.
        pub(crate) fn seek_to_ms(&self, ms: u32) {
            unsafe {
                let time = CMTime { value: ms as i64, timescale: 1000, flags: VALID, epoch: 0 };
                // Exactly there: the default snaps to a key frame (a clip with one goes back to its start).
                let zero = CMTime { value: 0, timescale: 1, flags: VALID, epoch: 0 };
                let _: () = msg_send![self.player, seekToTime: time toleranceBefore: zero toleranceAfter: zero];
            }
        }

        /// Current playback time in milliseconds.
        pub(crate) fn current_time_ms(&self) -> u32 {
            unsafe {
                let now: CMTime = msg_send![self.item, currentTime];
                if now.flags & VALID == 0 {
                    return 0;
                }
                let secs = CMTimeGetSeconds(now);
                if secs.is_finite() && secs > 0.0 {
                    (secs * 1000.0) as u32
                } else {
                    0
                }
            }
        }

        /// Whole length in milliseconds (0 until the system knows it).
        pub(crate) fn duration_ms(&self) -> u32 {
            unsafe {
                let total: CMTime = msg_send![self.item, duration];
                if total.flags & VALID == 0 {
                    return 0;
                }
                let secs = CMTimeGetSeconds(total);
                if secs.is_finite() && secs > 0.0 {
                    (secs * 1000.0) as u32
                } else {
                    0
                }
            }
        }

        pub(crate) fn is_playing(&self) -> bool {
            let rate: f32 = unsafe { msg_send![self.player, rate] };
            rate != 0.0
        }

        pub(crate) fn set_volume(&self, volume: f32) {
            unsafe {
                let _: () = msg_send![self.player, setVolume: volume];
            }
        }

        /// Whether it played to its end.
        pub(crate) fn finished(&self) -> bool {
            unsafe {
                let (now, total): (CMTime, CMTime) = (msg_send![self.item, currentTime], msg_send![self.item, duration]);
                if total.flags & VALID == 0 || now.flags & VALID == 0 {
                    return false;
                }
                let (now, total) = (CMTimeGetSeconds(now), CMTimeGetSeconds(total));
                total.is_finite() && total > 0.0 && now >= total - 0.05
            }
        }

        /// Whether it played to its end, taking trim_end_ms into account.
        pub(crate) fn finished_with_trim(&self, trim_end_ms: u32) -> bool {
            if trim_end_ms > 0 && self.current_time_ms() >= trim_end_ms {
                return true;
            }
            self.finished()
        }

        /// The newest frame, if there is one that was not taken yet.
        pub(crate) fn frame(&self) -> Option<Frame> {
            if self.output.is_null() {
                return None;
            }
            unsafe {
                let time: CMTime = msg_send![self.output, itemTimeForHostTime: CACurrentMediaTime()];
                let new: BOOL = msg_send![self.output, hasNewPixelBufferForItemTime: time];
                if new == objc::runtime::NO {
                    return None;
                }
                let buffer: *mut c_void = msg_send![self.output, copyPixelBufferForItemTime: time itemTimeForDisplay: std::ptr::null_mut::<CMTime>()];
                (!buffer.is_null()).then(|| CVPixelBuffer::wrap_under_create_rule(buffer as CVPixelBufferRef))
            }
        }
    }

    impl Drop for Player {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.player, pause];
                if !self.output.is_null() {
                    let _: () = msg_send![self.item, removeOutput: self.output];
                    let _: () = msg_send![self.output, release];
                }
                let _: () = msg_send![self.item, release];
                let _: () = msg_send![self.player, release];
            }
        }
    }
}

#[cfg(not(any(target_os = "macos",windows)))]
mod other {
    use std::path::Path;

    /// No frames are taken off macOS.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Frame;

    pub(crate) struct Player;

    pub(crate) fn video_aspect(_: &Path) -> Option<f64> {
        None
    }

    impl Player {
        pub(crate) fn open(_: &Path, _: bool) -> Option<Player> {
            None
        }
        pub(crate) fn play(&self) {}
        pub(crate) fn pause(&self) {}
        pub(crate) fn rewind(&self) {}
        pub(crate) fn seek_to_ms(&self, _ms: u32) {}
        pub(crate) fn current_time_ms(&self) -> u32 {
            0
        }
        pub(crate) fn set_volume(&self, _volume: f32) {}
        pub(crate) fn duration_ms(&self) -> u32 {
            0
        }
        pub(crate) fn is_playing(&self) -> bool {
            false
        }
        pub(crate) fn finished(&self) -> bool {
            true
        }
        pub(crate) fn finished_with_trim(&self, _trim_end_ms: u32) -> bool {
            true
        }
        pub(crate) fn frame(&self) -> Option<Frame> {
            None
        }
    }
}
