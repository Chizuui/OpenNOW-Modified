#!/usr/bin/env python3
"""Sub-split OpenNowStreamScreens.kt (7.3k lines) into feature files:
queue / touch controls / controls panel / bug reporter. The stream core
(StreamScreen, video surface, stats, guides, exit confirm) stays.

Same machinery as the OpenNowScreens.kt split: top-level declaration blocks,
annotation merging, moved decls private->internal, core decls referenced by
moved code -> internal, then deterministic unused-import cleanup per file.
"""
import os
import re
import sys

SRC = "app/src/main/java/com/opencloudgaming/opennow/OpenNowStreamScreens.kt"
OUT_DIR = "app/src/main/java/com/opencloudgaming/opennow/"

DECL_START = re.compile(r"^(?:@|private |internal |public )")
NAME_RE = re.compile(
    r"\b(?:enum class|data class|sealed class|sealed interface|annotation class|fun|val|class|object|interface|enum|typealias)\s+"
    r"([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)"
)

# name -> bucket. Anything not listed stays in `stream` core.
BUCKET = {
    # ---------------- QUEUE ----------------
    "QueueLoadingScreen": "queue",
    "QueueAmbientBackdrop": "queue",
    "QueueAmbientOrb": "queue",
    "QueueSignalField": "queue",
    "AnimatedQueueStatusText": "queue",
    "AnimatedQueueNumber": "queue",
    "QueueNumberDigitSlot": "queue",
    "rightAlignedCharAt": "queue",
    "QueueStatusParts": "queue",
    "queueStatusParts": "queue",
    "queueUrgency": "queue",
    "activeQueuePosition": "queue",
    "rememberStableQueuePosition": "queue",
    "queueLaunchStatusText": "queue",
    "queueIdleStatusColor": "queue",
    "queueUrgencyColor": "queue",
    "QueueStatusPanel": "queue",
    "LandscapeQueuePositionDock": "queue",
    "QueueAdPanel": "queue",
    "QueueAdPlayback": "queue",
    "QueueAdHeading": "queue",
    "QueueStatusAndActions": "queue",
    "MinimizedQueueDock": "queue",
    "MinimizedQueueStatusText": "queue",
    "QueueAdPlayer": "queue",
    "QueueAdControlIcon": "queue",
    "QueueAdIconButton": "queue",
    "QueueAdControlIconView": "queue",
    "PrintedWasteSelector": "queue",
    "PrintedWasteGameSummary": "queue",
    "PrintedWasteOptionsColumn": "queue",
    "RecommendedPrintedWasteCard": "queue",
    "PrintedWasteZoneRow": "queue",
    "QueueMetricPill": "queue",
    "isStandardPrintedWasteZone": "queue",
    "PrintedWasteZoneOption": "queue",
    "recommendedPrintedWasteZone": "queue",
    "printedWasteScore": "queue",
    "printedWasteZoneUrl": "queue",
    "formatPrintedWasteWait": "queue",
    "queueColor": "queue",
    "pingColor": "queue",
    "regionLabel": "queue",
    # ---------------- TOUCH CONTROLS ----------------
    "TouchOverlay": "touch",
    "PortraitTouchControls": "touch",
    "LandscapeTouchControls": "touch",
    "landscapeTouchTopControlClearanceDp": "touch",
    "TouchControlGroup": "touch",
    "clampStickOffset": "touch",
    "applyTouchJoystickDeadZone": "touch",
    "VirtualStick": "touch",
    "FaceButtonCluster": "touch",
    "DpadArrowhead": "touch",
    "DpadCluster": "touch",
    "virtualPressInput": "touch",
    "GamepadTriggerButton": "touch",
    "GamepadBumperButton": "touch",
    "GamepadButton": "touch",
    # ---------------- CONTROLS PANEL ----------------
    "StreamControlsPage": "panel",
    "StreamControlsPanel": "panel",
    "StreamPanelHeader": "panel",
    "StreamPanelHeaderButton": "panel",
    "streamPanelPageTransition": "panel",
    "StreamPanelKeyButton": "panel",
    "TouchLayoutSlider": "panel",
    "onOffLabel": "panel",
    "SHARPENING_SLIDER_STEP": "panel",
    "TOUCH_SCALE_SLIDER_STEP": "panel",
    "TOUCH_DP_SLIDER_STEP": "panel",
    "JOYSTICK_DEAD_ZONE_STEP": "panel",
    "DP_UNIT": "panel",
    "StreamKeyboardBar": "panel",
    "MAX_STREAM_KEYBOARD_TEXT_LENGTH": "panel",
    "mouseModePageItems": "panel",
    "statusBarPageItems": "panel",
    # ---------------- BUG REPORTER ----------------
    "BugReportDataDisclosure": "bugreport",
    "BugReportSubmissionRequirements": "bugreport",
    "BugReportVersionGateCard": "bugreport",
    "BugReportPreflightDeckView": "bugreport",
    "BugReportFormInputs": "bugreport",
    "StreamBugReporter": "bugreport",
}


def decl_name(lines, start, end, limit=8):
    for i in range(start, min(start + limit, end)):
        line = lines[i]
        if line.startswith("@"):
            continue
        m = NAME_RE.search(line)
        if m:
            return m.group(1).split(".")[-1]
    return ""


def split_blocks(lines):
    starts = [i for i, line in enumerate(lines) if DECL_START.match(line)]
    raw = []
    for k, s in enumerate(starts):
        e = starts[k + 1] if k + 1 < len(starts) else len(lines)
        raw.append([s, e])
    merged = []
    pending_start = None
    for s, e in raw:
        if decl_name(lines, s, e):
            if pending_start is not None:
                s = pending_start
                pending_start = None
            merged.append([s, e])
        else:
            if pending_start is None:
                pending_start = s
    if pending_start is not None:
        merged.append([pending_start, len(lines)])
    out = []
    for s, e in merged:
        while e > s and not lines[e - 1].strip():
            e -= 1
        out.append((s, e))
    return out


def main():
    with open(SRC) as f:
        text = f.read()
    lines = text.split("\n")
    import_block = "\n".join(ln for ln in lines if ln.startswith("import "))

    blocks = split_blocks(lines)
    assignments = []
    seen = {}
    for s, e in blocks:
        name = decl_name(lines, s, e)
        if not name:
            print(f"WARN: no name for block at line {s + 1}", file=sys.stderr)
            continue
        if name in seen:
            print(f"WARN: duplicate name {name} at line {s + 1} (also {seen[name]})", file=sys.stderr)
        seen[name] = s + 1
        bucket = BUCKET.get(name, "stream")
        assignments.append((bucket, s, e, name))

    moved_text = "\n".join(
        "\n".join(lines[s:e]) for b, s, e, n in assignments if b != "stream"
    )
    core_names = {n for b, s, e, n in assignments if b == "stream"}

    def needs_internal(name):
        return BUCKET.get(name) not in (None, "stream")

    def core_referenced(name):
        return name in core_names and re.search(rf"\b{re.escape(name)}\b", moved_text) is not None

    buckets = {}
    for b, s, e, n in assignments:
        block_lines = list(lines[s:e])
        if needs_internal(n) or core_referenced(n):
            for idx, bl in enumerate(block_lines):
                if bl.startswith("private "):
                    block_lines[idx] = "internal " + bl[len("private "):]
                    break
        buckets.setdefault(b, []).append(block_lines)

    for b in ["stream", "queue", "touch", "panel", "bugreport"]:
        entries = buckets.get(b, [])
        if not entries:
            continue
        out_path = SRC if b == "stream" else OUT_DIR + {
            "queue": "OpenNowQueueScreens.kt",
            "touch": "OpenNowTouchControls.kt",
            "panel": "OpenNowStreamControlsPanel.kt",
            "bugreport": "OpenNowBugReportScreens.kt",
        }[b]
        parts = ["package com.opencloudgaming.opennow\n", import_block, "\n"]
        for block_lines in entries:
            parts.append("\n".join(block_lines).rstrip())
        with open(out_path, "w") as f:
            f.write("\n\n".join(parts) + "\n")
        print(f"{out_path}: {len(entries)} declarations ({sum(len(x) for x in entries)} lines)")

    # ---- deterministic unused-import cleanup ----
    IMPORT_RE = re.compile(r"^import (\S+?)(?: as (\w+))?$")
    ALWAYS_KEEP = {"getValue", "setValue"}

    def symbol_for(imp):
        m = IMPORT_RE.match(imp)
        if not m:
            return None
        path, alias = m.group(1), m.group(2)
        if path.endswith(".*"):
            return None
        return alias if alias else path.split(".")[-1]

    targets = [SRC] + [
        OUT_DIR + f
        for f in ["OpenNowQueueScreens.kt", "OpenNowTouchControls.kt",
                  "OpenNowStreamControlsPanel.kt", "OpenNowBugReportScreens.kt"]
    ]
    for path in targets:
        with open(path) as f:
            tlines = f.read().split("\n")
        body = "\n".join(l for l in tlines if not l.startswith("import "))
        out = []
        removed = 0
        for line in tlines:
            if line.startswith("import "):
                sym = symbol_for(line)
                if sym is not None and sym not in ALWAYS_KEEP and not re.search(
                    rf"\b{re.escape(sym)}\b", body
                ):
                    removed += 1
                    continue
            out.append(line)
        with open(path, "w") as f:
            f.write("\n".join(out))
        print(f"imports cleaned {path}: removed {removed}")

    print("TOTAL:", len(assignments), "blocks")


if __name__ == "__main__":
    main()
