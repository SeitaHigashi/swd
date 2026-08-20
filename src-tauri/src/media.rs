//! Now-playing card backend: reads title/artist/artwork/playback state from
//! the Windows System Media Transport Controls (the same API backing the
//! Win+G / lock screen media overlay) and forwards transport commands to
//! whichever app currently owns the session (Spotify, browser tab, etc).
//!
//! WinRT activation requires COM to be initialized on the calling thread,
//! so both polling and commands run on one dedicated background thread
//! (commands arrive over a channel) rather than on Tauri's shared async
//! command threads, where the apartment state isn't guaranteed.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as MediaSession,
    GlobalSystemMediaTransportControlsSessionManager as MediaSessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

const POLL_INTERVAL: Duration = Duration::from_millis(1000);
const NOW_PLAYING_EVENT: &str = "media://now-playing";
/// Guard against a pathological thumbnail stream; real album art is a few
/// hundred KB at most.
const MAX_THUMBNAIL_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct NowPlaying {
    title: String,
    artist: String,
    status: String,
    thumbnail_data_url: Option<String>,
}

enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
}

static COMMAND_TX: Mutex<Option<Sender<MediaCommand>>> = Mutex::new(None);

fn playback_status_label(status: PlaybackStatus) -> &'static str {
    match status {
        PlaybackStatus::Playing => "playing",
        PlaybackStatus::Paused => "paused",
        PlaybackStatus::Stopped => "stopped",
        _ => "unknown",
    }
}

fn read_thumbnail(properties: &MediaProperties) -> Option<String> {
    let thumb_ref = properties.Thumbnail().ok()?;
    let stream = thumb_ref.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > MAX_THUMBNAIL_BYTES {
        return None;
    }

    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size as u32).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;

    let content_type = stream
        .ContentType()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|_| "image/png".to_string());

    Some(format!("data:{content_type};base64,{}", BASE64.encode(buf)))
}

fn current_session() -> Option<MediaSession> {
    let manager = MediaSessionManager::RequestAsync().ok()?.get().ok()?;
    manager.GetCurrentSession().ok()
}

fn poll_now_playing() -> Option<NowPlaying> {
    let session = current_session()?;
    let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let status = session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()?;

    let title = properties.Title().map(|s| s.to_string_lossy()).unwrap_or_default();
    if title.is_empty() {
        return None;
    }

    Some(NowPlaying {
        title,
        artist: properties.Artist().map(|s| s.to_string_lossy()).unwrap_or_default(),
        status: playback_status_label(status).to_string(),
        thumbnail_data_url: read_thumbnail(&properties),
    })
}

fn apply_command(command: MediaCommand) {
    let Some(session) = current_session() else {
        return;
    };
    let result = match command {
        MediaCommand::TogglePlayPause => session.TryTogglePlayPauseAsync().and_then(|op| op.get()),
        MediaCommand::Next => session.TrySkipNextAsync().and_then(|op| op.get()),
        MediaCommand::Previous => session.TrySkipPreviousAsync().and_then(|op| op.get()),
    };
    if let Err(err) = result {
        eprintln!("[media] transport command failed: {err:?}");
    }
}

/// Starts the dedicated COM/WinRT thread: polls now-playing state on an
/// interval and executes transport commands sent from `send_command` as
/// they arrive, re-emitting state immediately after each command so the UI
/// feels responsive.
pub fn start_media_monitor(app: AppHandle) {
    let (tx, rx) = mpsc::channel::<MediaCommand>();
    *COMMAND_TX.lock().unwrap() = Some(tx);

    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        loop {
            match rx.recv_timeout(POLL_INTERVAL) {
                Ok(command) => apply_command(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
        }
    });
}

fn send_command(command: MediaCommand) {
    if let Some(tx) = COMMAND_TX.lock().unwrap().as_ref() {
        let _ = tx.send(command);
    }
}

#[tauri::command]
pub fn media_toggle_play_pause() {
    send_command(MediaCommand::TogglePlayPause);
}

#[tauri::command]
pub fn media_next() {
    send_command(MediaCommand::Next);
}

#[tauri::command]
pub fn media_previous() {
    send_command(MediaCommand::Previous);
}
