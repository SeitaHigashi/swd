//! Now-playing card backend: reads title/artist/artwork/playback state from
//! the Windows System Media Transport Controls (the same API backing the
//! Win+G / lock screen media overlay) and forwards transport commands to
//! whichever app currently owns the session (Spotify, browser tab, etc).
//!
//! WinRT activation requires COM to be initialized on the calling thread,
//! so both event handling and commands run on one dedicated background
//! thread (commands arrive over a channel) rather than on Tauri's shared
//! async command threads, where the apartment state isn't guaranteed.
//!
//! State updates are event-driven rather than polled: we subscribe to the
//! session manager's `SessionsChanged` event (fires when the foreground
//! session changes) and, on the current session, `MediaPropertiesChanged`
//! / `PlaybackInfoChanged` / `TimelinePropertiesChanged`. These are WinRT
//! `TypedEventHandler` callbacks that WinRT invokes from its own worker
//! threads (this thread runs MTA via `COINIT_MULTITHREADED`, so there is
//! no STA message loop to pump them on). To keep every touch of the WinRT
//! objects and every emit on the one dedicated thread — preserving the
//! single-thread discipline described above — the callbacks don't call
//! `app.emit` or touch the session objects directly. They just push a
//! lightweight signal into the same `mpsc` channel that already carries
//! transport commands, and the dedicated thread's loop does the actual
//! WinRT calls and emit in response.
//!
//! A coarse fallback poll (`FALLBACK_POLL_INTERVAL`) remains as a safety
//! net: not every app implements SMTC events reliably, so this re-checks
//! state periodically even if no event fires.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as MediaSession,
    GlobalSystemMediaTransportControlsSessionManager as MediaSessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Safety-net poll interval. Primary state updates come from SMTC events;
/// this only exists to catch apps whose sessions don't fire them reliably.
const FALLBACK_POLL_INTERVAL: Duration = Duration::from_secs(8);
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

/// Everything that can wake the dedicated thread's loop: a transport
/// command from the frontend, or a signal pushed by a WinRT event
/// callback running on some other (WinRT-owned) thread.
enum MediaEvent {
    Command(MediaCommand),
    /// The session manager's foreground session changed; re-subscribe.
    SessionsChanged,
    /// The current session's properties/playback/timeline changed.
    SessionStateChanged,
}

static COMMAND_TX: Mutex<Option<Sender<MediaEvent>>> = Mutex::new(None);

/// Registration tokens for the three per-session events, kept together so
/// they can be unsubscribed as a unit when the session changes or when the
/// thread shuts down. `windows` 0.61's `Media_Control` bindings represent
/// event registration tokens as plain `i64`s.
struct SessionTokens {
    media_properties: i64,
    playback_info: i64,
    timeline_properties: i64,
}

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

/// Subscribes to `session`'s three state-change events, each pushing a
/// `SessionStateChanged` signal into `tx`. The callbacks run on a WinRT
/// worker thread, not this one — they must not touch WinRT objects or
/// `AppHandle` themselves, only send.
///
/// On partial failure (registering the second or third handler errors out)
/// unwinds anything already registered so we don't leak a live callback
/// referencing a session we're about to drop.
fn subscribe_session(session: &MediaSession, tx: &Sender<MediaEvent>) -> Option<SessionTokens> {
    // Each event has a distinct args type, so each needs its own
    // `TypedEventHandler::new` monomorphization — a closure can't be
    // reused generically across them the way a function could.
    let media_properties = {
        let tx = tx.clone();
        session
            .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = tx.send(MediaEvent::SessionStateChanged);
                Ok(())
            }))
            .ok()?
    };

    let playback_info = {
        let tx = tx.clone();
        match session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            let _ = tx.send(MediaEvent::SessionStateChanged);
            Ok(())
        })) {
            Ok(token) => token,
            Err(err) => {
                eprintln!("[media] failed to subscribe PlaybackInfoChanged: {err:?}");
                let _ = session.RemoveMediaPropertiesChanged(media_properties);
                return None;
            }
        }
    };

    let timeline_properties = {
        let tx = tx.clone();
        match session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = tx.send(MediaEvent::SessionStateChanged);
            Ok(())
        })) {
            Ok(token) => token,
            Err(err) => {
                eprintln!("[media] failed to subscribe TimelinePropertiesChanged: {err:?}");
                let _ = session.RemoveMediaPropertiesChanged(media_properties);
                let _ = session.RemovePlaybackInfoChanged(playback_info);
                return None;
            }
        }
    };

    Some(SessionTokens {
        media_properties,
        playback_info,
        timeline_properties,
    })
}

fn unsubscribe_session(session: &MediaSession, tokens: SessionTokens) {
    let _ = session.RemoveMediaPropertiesChanged(tokens.media_properties);
    let _ = session.RemovePlaybackInfoChanged(tokens.playback_info);
    let _ = session.RemoveTimelinePropertiesChanged(tokens.timeline_properties);
}

/// Drops the current session subscription (if any) and re-subscribes to
/// whatever `manager.GetCurrentSession()` returns now. Called both at
/// startup and every time `SessionsChanged` fires.
fn resubscribe_current_session(
    manager: &MediaSessionManager,
    tx: &Sender<MediaEvent>,
    current: &mut Option<(MediaSession, SessionTokens)>,
) {
    if let Some((old_session, old_tokens)) = current.take() {
        unsubscribe_session(&old_session, old_tokens);
    }
    *current = manager
        .GetCurrentSession()
        .ok()
        .and_then(|session| subscribe_session(&session, tx).map(|tokens| (session, tokens)));
}

/// Starts the dedicated COM/WinRT thread: subscribes to SMTC session
/// events (falling back to a coarse poll if events aren't available or
/// don't fire), executes transport commands sent from `send_command` as
/// they arrive, and emits fresh state after every event/command/poll tick.
pub fn start_media_monitor(app: AppHandle) {
    let (tx, rx) = mpsc::channel::<MediaEvent>();
    *COMMAND_TX.lock().unwrap() = Some(tx.clone());

    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        let manager = MediaSessionManager::RequestAsync().ok().and_then(|op| op.get().ok());

        let Some(manager) = manager else {
            // No session manager available at all (unusual, but possible
            // in a locked-down environment) — fall back to poll-only so
            // the card at least still works, just without instant updates.
            eprintln!("[media] failed to acquire session manager; falling back to poll-only mode");
            loop {
                match rx.recv_timeout(FALLBACK_POLL_INTERVAL) {
                    Ok(MediaEvent::Command(command)) => {
                        apply_command(command);
                        let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
                    }
                    Ok(_) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        };

        let sessions_changed_token = {
            let tx = tx.clone();
            manager
                .SessionsChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = tx.send(MediaEvent::SessionsChanged);
                    Ok(())
                }))
                .ok()
        };
        if sessions_changed_token.is_none() {
            eprintln!("[media] failed to subscribe SessionsChanged; relying on fallback poll only");
        }

        let mut current: Option<(MediaSession, SessionTokens)> = None;
        resubscribe_current_session(&manager, &tx, &mut current);

        // Emit initial state immediately rather than waiting for the first
        // event or fallback poll tick.
        let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());

        loop {
            match rx.recv_timeout(FALLBACK_POLL_INTERVAL) {
                Ok(MediaEvent::Command(command)) => {
                    apply_command(command);
                    let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
                }
                Ok(MediaEvent::SessionStateChanged) => {
                    let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
                }
                Ok(MediaEvent::SessionsChanged) => {
                    resubscribe_current_session(&manager, &tx, &mut current);
                    let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
                }
                // Safety net: some apps don't fire SMTC events reliably,
                // so re-check periodically even without a signal.
                Err(RecvTimeoutError::Timeout) => {
                    let _ = app.emit(NOW_PLAYING_EVENT, poll_now_playing());
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        if let Some(token) = sessions_changed_token {
            let _ = manager.RemoveSessionsChanged(token);
        }
        if let Some((session, tokens)) = current {
            unsubscribe_session(&session, tokens);
        }
    });
}

fn send_command(command: MediaCommand) {
    if let Some(tx) = COMMAND_TX.lock().unwrap().as_ref() {
        let _ = tx.send(MediaEvent::Command(command));
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
