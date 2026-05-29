use std::str::FromStr;

use cap_recording::{screenshot::capture_screenshot, sources::screen_capture::ScreenCaptureTarget};
use clipboard_rs::{Clipboard, ClipboardContext};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    MutableState,
    recording_settings::RecordingTargetMode,
    screenshot_editor::{ScreenshotOcrRegion, recognize_screenshot_text_from_rgba},
    target_select_overlay::{self, WindowFocusManager},
    windows::{CapWindowId, TargetSelectAction, hide_overlay},
};

#[derive(Clone, Serialize, Type, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OcrCaptureClipboardResult {
    pub text: String,
    pub engine: String,
    pub line_count: usize,
}

#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(app, state))]
pub async fn open_ocr_capture_to_clipboard(
    app: AppHandle,
    state: tauri::State<'_, WindowFocusManager>,
) -> Result<(), String> {
    if let Some(window) = CapWindowId::Main.get(&app) {
        let _ = window.hide();
    }

    let mut closed_display_ids = Vec::new();
    for (id, window) in app.webview_windows() {
        if let Ok(CapWindowId::TargetSelectOverlay { display_id }) = CapWindowId::from_str(&id) {
            hide_overlay(&window);
            closed_display_ids.push(display_id);
        }
    }

    for display_id in closed_display_ids {
        state.destroy(&display_id, app.global_shortcut());
    }

    target_select_overlay::open_target_select_overlays_with_action(
        app,
        state,
        None,
        None,
        Some(RecordingTargetMode::Area),
        Some(TargetSelectAction::OcrToClipboard),
    )
    .await
}

#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(app, clipboard, target))]
pub async fn capture_screenshot_text_to_clipboard(
    app: AppHandle,
    clipboard: MutableState<'_, ClipboardContext>,
    target: ScreenCaptureTarget,
) -> Result<OcrCaptureClipboardResult, String> {
    hide_capture_overlays(&app).await;

    let image = capture_screenshot(target)
        .await
        .map_err(|e| format!("Failed to capture screenshot for OCR: {e}"))?;
    let width = image.width();
    let height = image.height();
    let source_rgba = image.to_rgba8().into_raw();
    let result = recognize_screenshot_text_from_rgba(
        &source_rgba,
        width,
        height,
        ScreenshotOcrRegion {
            x: 0,
            y: 0,
            width,
            height,
        },
    )
    .await?;
    let text = result.text.trim().to_string();

    if text.is_empty() {
        return Err("No text found in selection".to_string());
    }

    clipboard
        .write()
        .await
        .set_text(text.clone())
        .map_err(|err| format!("Failed to copy OCR text to clipboard: {err}"))?;

    Ok(OcrCaptureClipboardResult {
        text,
        engine: result.engine,
        line_count: result.lines.len(),
    })
}

async fn hide_capture_overlays(app: &AppHandle) {
    let mut hid_any = false;
    for (label, window) in app.webview_windows() {
        if let Ok(id) = CapWindowId::from_str(&label)
            && matches!(
                id,
                CapWindowId::TargetSelectOverlay { .. }
                    | CapWindowId::WindowCaptureOccluder { .. }
                    | CapWindowId::CaptureArea
                    | CapWindowId::ModeSelect
                    | CapWindowId::RecordingsOverlay
            )
        {
            hide_overlay(&window);
            hid_any = true;
        }
    }

    if hid_any {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
}
