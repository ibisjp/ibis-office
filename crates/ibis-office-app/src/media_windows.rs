// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Windows' native MediaPlayer. Audio uses its playback clock; video is
//! copied from the frame-server surface into the application's image layer.
use std::{
    cell::RefCell,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Foundation::{TimeSpan, TypedEventHandler, Uri},
    Graphics::DirectX::Direct3D11::IDirect3DSurface,
    Media::{
        Core::MediaSource,
        Playback::{MediaPlaybackState, MediaPlayer},
    },
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::*,
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGISurface,
            },
        },
        System::WinRT::{
            Direct3D11::CreateDirect3D11SurfaceFromDXGISurface, RO_INIT_MULTITHREADED,
            RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
        },
    },
    core::{HSTRING, Interface, Result},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}
pub(crate) struct Player {
    player: MediaPlayer,
    ended: Arc<AtomicBool>,
    available: Arc<AtomicBool>,
    ended_token: i64,
    frame_token: Option<i64>,
    video: bool,
    output: RefCell<Option<Output>>,
    _apartment: Apartment,
}
struct Apartment;
impl Apartment {
    fn new() -> Result<Self> {
        unsafe {
            if RoInitialize(RO_INIT_MULTITHREADED).is_err() {
                RoInitialize(RO_INIT_SINGLETHREADED)?;
            }
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}
struct Output {
    width: u32,
    height: u32,
    _device: ID3D11Device,
    context: ID3D11DeviceContext,
    texture: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    surface: IDirect3DSurface,
}
impl Output {
    fn new(width: u32, height: u32) -> Result<Self> {
        unsafe {
            let (mut device, mut context) = (None, None);
            let create = |kind, device: &mut Option<_>, context: &mut Option<_>| {
                D3D11CreateDevice(
                    None,
                    kind,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(device),
                    None,
                    Some(context),
                )
            };
            create(D3D_DRIVER_TYPE_HARDWARE, &mut device, &mut context)
                .or_else(|_| create(D3D_DRIVER_TYPE_WARP, &mut device, &mut context))?;
            let device = device.unwrap();
            let context = context.unwrap();
            let mut desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                ..Default::default()
            };
            let mut texture = None;
            device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.unwrap();
            let surface: IDirect3DSurface =
                CreateDirect3D11SurfaceFromDXGISurface(&texture.cast::<IDXGISurface>()?)?.cast()?;
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            device.CreateTexture2D(&desc, None, Some(&mut staging))?;
            Ok(Self {
                width,
                height,
                _device: device,
                context,
                texture,
                staging: staging.unwrap(),
                surface,
            })
        }
    }
    fn copy(&self, player: &MediaPlayer) -> Result<Frame> {
        unsafe {
            player.CopyFrameToVideoSurface(&self.surface)?;
            self.context.CopyResource(&self.staging, &self.texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut rgba = vec![0; self.width as usize * self.height as usize * 4];
            for y in 0..self.height as usize {
                let src = std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                    self.width as usize * 4,
                );
                for (x, pixel) in src.chunks_exact(4).enumerate() {
                    let at = (y * self.width as usize + x) * 4;
                    rgba[at..at + 4].copy_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
                }
            }
            self.context.Unmap(&self.staging, 0);
            Ok(Frame {
                width: self.width,
                height: self.height,
                rgba: Arc::new(rgba),
            })
        }
    }
}
fn file_uri(path: &Path) -> Option<String> {
    let canonical = path.canonicalize().ok()?;
    let raw = canonical.to_string_lossy().replace('\\', "/");
    let raw = raw
        .strip_prefix("//?/UNC/")
        .map(|p| format!("//{p}"))
        .unwrap_or_else(|| raw.strip_prefix("//?/").unwrap_or(&raw).to_owned());
    let mut encoded = String::new();
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            encoded.push(b as char);
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{b:02X}").ok()?;
        }
    }
    Some(if encoded.starts_with("//") {
        format!("file:{encoded}")
    } else {
        format!("file:///{encoded}")
    })
}
impl Player {
    pub(crate) fn open(path: &Path, video: bool) -> Option<Self> {
        Self::open_native(path, video)
            .inspect_err(|e| log::warn!("Cannot initialize native media playback: {e}"))
            .ok()
    }
    fn open_native(path: &Path, video: bool) -> Result<Self> {
        let apartment = Apartment::new()?;
        let player = MediaPlayer::new()?;
        player.SetAutoPlay(false)?;
        let ended = Arc::new(AtomicBool::new(false));
        let event_ended = ended.clone();
        let ended_token = player.MediaEnded(&TypedEventHandler::new(move |_, _| {
            event_ended.store(true, Ordering::Release);
            Ok(())
        }))?;
        let available = Arc::new(AtomicBool::new(false));
        let frame_token = if video {
            let ready = available.clone();
            player.SetIsVideoFrameServerEnabled(true)?;
            Some(
                player.VideoFrameAvailable(&TypedEventHandler::new(move |_, _| {
                    ready.store(true, Ordering::Release);
                    Ok(())
                }))?,
            )
        } else {
            None
        };
        let uri = file_uri(path).ok_or_else(windows::core::Error::from_win32)?;
        player.SetSource(&MediaSource::CreateFromUri(&Uri::CreateUri(
            &HSTRING::from(uri),
        )?)?)?;
        Ok(Self {
            player,
            ended,
            available,
            ended_token,
            frame_token,
            video,
            output: RefCell::new(None),
            _apartment: apartment,
        })
    }
    pub(crate) fn play(&self) {
        self.ended.store(false, Ordering::Release);
        self.player.Play().ok();
    }
    pub(crate) fn pause(&self) {
        self.player.Pause().ok();
    }
    pub(crate) fn rewind(&self) {
        self.seek_to_ms(0);
    }
    pub(crate) fn seek_to_ms(&self, ms: u32) {
        self.ended.store(false, Ordering::Release);
        if let Ok(s) = self.player.PlaybackSession() {
            s.SetPosition(TimeSpan {
                Duration: i64::from(ms) * 10_000,
            })
            .ok();
        }
    }
    pub(crate) fn current_time_ms(&self) -> u32 {
        self.player
            .PlaybackSession()
            .and_then(|s| s.Position())
            .map_or(0, |t| {
                (t.Duration.max(0) / 10_000).min(u32::MAX as i64) as u32
            })
    }
    /// Whole length in milliseconds (0 until the system knows it).
    pub(crate) fn duration_ms(&self) -> u32 {
        self.player
            .PlaybackSession()
            .and_then(|s| s.NaturalDuration())
            .map_or(0, |t| {
                (t.Duration.max(0) / 10_000).min(u32::MAX as i64) as u32
            })
    }
    pub(crate) fn set_volume(&self, volume: f32) {
        self.player.SetVolume(f64::from(volume.clamp(0., 1.))).ok();
    }
    pub(crate) fn is_playing(&self) -> bool {
        self.player
            .PlaybackSession()
            .and_then(|s| s.PlaybackState())
            .is_ok_and(|s| s == MediaPlaybackState::Playing)
    }
    pub(crate) fn finished(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }
    pub(crate) fn finished_with_trim(&self, end: u32) -> bool {
        self.finished() || (end > 0 && self.current_time_ms() >= end)
    }
    pub(crate) fn frame(&self) -> Option<Frame> {
        if !self.video || !self.available.swap(false, Ordering::AcqRel) {
            return None;
        }
        let session = self.player.PlaybackSession().ok()?;
        let (w, h) = (
            session.NaturalVideoWidth().ok()?,
            session.NaturalVideoHeight().ok()?,
        );
        if w == 0 || h == 0 {
            return None;
        }
        let mut output = self.output.borrow_mut();
        if output
            .as_ref()
            .is_none_or(|o| o.width != w || o.height != h)
        {
            *output = Output::new(w, h).ok();
        }
        output.as_ref()?.copy(&self.player).ok()
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.player.RemoveMediaEnded(self.ended_token).ok();
        if let Some(t) = self.frame_token {
            self.player.RemoveVideoFrameAvailable(t).ok();
        }
        self.player.Close().ok();
    }
}
pub(crate) fn video_aspect(path: &Path) -> Option<f64> {
    let player = Player::open(path, true)?;
    let session = player.player.PlaybackSession().ok()?;
    let (w, h) = (
        session.NaturalVideoWidth().ok()?,
        session.NaturalVideoHeight().ok()?,
    );
    (w > 0 && h > 0).then_some(w as f64 / h as f64)
}
