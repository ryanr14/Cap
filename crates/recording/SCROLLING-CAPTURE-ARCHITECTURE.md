# Scrolling Capture Architecture Spike

## Current Capture Path

Target selection lives in `apps/desktop/src/routes/target-select-overlay.tsx` and the native overlay support in `apps/desktop/src-tauri/src/target_select_overlay.rs`. The selected target is still a `ScreenCaptureTarget` variant: display, window, area, or camera-only. Screenshot mode reuses the same target overlay and calls the native `take_screenshot` command instead of starting a recording.

`apps/desktop/src-tauri/src/recording.rs::take_screenshot` hides Cap overlay windows, calls `cap_recording::screenshot::capture_screenshot`, writes an `original.png` inside a generated `.cap` screenshot directory, saves default project config plus recording metadata, and returns the image path. A short-lived `PendingScreenshots` entry lets the editor open before PNG encoding has finished.

`crates/recording/src/screenshot.rs::capture_screenshot` is a single visible-region capture. On macOS it first tries CoreGraphics display/window capture and then falls back to ScreenCaptureKit. On Windows it first tries the Direct3D path, then GDI/window fallbacks. None of those paths expose scroll container identity, page height, or scroll offset.

The screenshot editor already accepts either a plain image path or a screenshot `.cap` directory. `apps/desktop/src-tauri/src/screenshot_editor.rs` loads `original.png` from `.cap` directories, loads direct PNG paths, or consumes pending raw screenshot bytes. The editor has `MAX_DIMENSION = 16_384`, so tall stitched captures need preflight validation before opening.

## Why Broad Native Autoscroll Is Not A Safe First Slice

Native autoscroll needs an input driver and a verifier, not just repeated `capture_screenshot` calls. The current target model can identify the visible display/window/area, but it does not identify the scrollable element inside that target. App-specific scroll behavior, inertial scrolling, sticky headers, lazy-loaded content, cursor position, and accessibility permission prompts all affect correctness. A broad implementation would touch target selection, global input synthesis, screenshot timing, stitch validation, metadata, editor limits, and error recovery at once.

The lower-risk first branch is a manual capture/stitch loop that proves image handling, output shape, and editor import without synthesizing scroll input.

## Prototype Added In This Spike

This branch adds an explicit CLI-only prototype command:

```bash
cargo run -p cap -- scrolling-capture-stitch \
  --output /tmp/cap-scrolling-capture.png \
  --max-overlap 900 \
  /path/to/scroll-001.png \
  /path/to/scroll-002.png \
  /path/to/scroll-003.png
```

The command loads two or more same-width images, detects repeated bottom/top overlap between adjacent captures, trims matched rows, and writes one vertical PNG. It prints the output size plus per-image trim decisions. This is intentionally not wired into the desktop UI or default screenshot flow.

Suggested manual validation:

1. Use the existing screenshot mode to capture a visible area or window.
2. Manually scroll the content, keeping the same target width.
3. Capture the next visible frame.
4. Run `scrolling-capture-stitch` with the captured `original.png` paths or exported PNGs.
5. Import the stitched PNG through the existing desktop "Import image" surface, or open it directly in the screenshot editor path flow during a later desktop integration branch.

## Recommended Architecture

Phase 1 should promote the stitcher into a shared Rust module instead of leaving the algorithm in the CLI. The desktop command can then call the same code and keep the CLI as the fast regression harness. The shared API should accept already-decoded RGBA frames, a target width, overlap thresholds, and a maximum output dimension.

Phase 2 should add an explicit desktop-only manual scrolling capture surface. A safe flow would be:

1. User chooses "Scrolling screenshot" from an experimental command or deeplink.
2. Cap reuses the existing target selector for display/window/area selection.
3. Cap captures the first visible frame with `capture_screenshot`.
4. Cap shows a small non-default overlay with "Capture next" and "Finish".
5. User scrolls manually between captures.
6. Cap stitches frames, writes a normal screenshot `.cap` directory with `original.png`, saves metadata/config through the existing screenshot path, and opens `ShowCapWindow::ScreenshotEditor`.

Phase 3 can investigate guided scrolling. Keep this behind an experimental setting until the branch proves app behavior across browsers, Electron apps, native scroll views, and multi-display scaling.

Phase 4 can evaluate native autoscroll providers. macOS likely needs Accessibility APIs for focused element discovery and input, while Windows likely needs UI Automation plus wheel input fallback. Both providers need a stop condition based on image overlap, scroll offset, or stable end-of-content detection.

## Data Model Additions

A stitched `.cap` screenshot can keep the existing `original.png` editor contract and add an optional manifest later:

```json
{
  "kind": "scrollingScreenshot",
  "version": 1,
  "target": "window|area|display",
  "captures": [
    { "path": "captures/001.png", "trimTop": 0 },
    { "path": "captures/002.png", "trimTop": 312 }
  ],
  "stitched": { "path": "original.png", "width": 1440, "height": 4200 }
}
```

The editor does not need this manifest for first support. It is useful for debugging, future restitching, and showing capture diagnostics.

## Risks And Guardrails

- Enforce the screenshot editor's `16_384` dimension limit before opening a stitched result.
- Keep manual and guided flows behind an explicit command, deeplink, or experimental UI until overlap detection is reliable.
- Do not capture Cap overlay windows. Reuse the existing overlay hide/ignore-cursor pattern from `take_screenshot`.
- Keep per-frame raw buffers bounded. A 4K by 16K RGBA image is roughly 256 MB before editor/rendering copies.
- Treat sticky headers and animated content as expected mismatch cases. The stitcher should report no trim instead of guessing aggressively.
- Preserve same-width capture requirements in the first implementation. Mixed-scale stitching should be a separate branch.

## Recommended First Branch

Move the CLI stitch logic into a small `crates/recording` or new `crates/image-stitching` module, then add a desktop command that accepts image paths, stitches them, writes a screenshot `.cap` directory, and opens the existing screenshot editor. Keep the command unreachable from default UI and document a deeplink or debug invocation for validation.
