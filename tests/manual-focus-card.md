# Focus card: manual acceptance

## Disposable preview

From the repository root, on an Omarchy Wayland desktop:

```bash
make backend
python tests/ui-check.py --preview --hour-demo
```

This opens the real view and CLI with a temporary ledger/home and a copy of the
current theme. It starts a task at **59:45**. Leave it running: within about 20
seconds, one wooden click and a milestone message should appear. The preview
does not use your ledger or Drive remote. Its files are removed when it closes.
Omit `--hour-demo` for a normal 10-minute sample. Theme changes after launching
require restarting this disposable preview; the installed widget follows the
live shell theme instead.

## Keyboard and task flow

1. Open the installed widget with `omarchy-shell omatracker open`. The primary
   Pause/Resume control has a visible keyboard outline. Space pauses/resumes.
2. Use arrows or `j/k` to select tasks. The hero timer stays on the active task;
   Space affects the visibly selected task. Click Pause with the mouse, then
   navigate to another task by keyboard and verify Space affects the new target.
3. Press `n`, type a name containing `j k r d`, and save with Enter. Those letters
   must not navigate, reset, or delete anything. New tasks start paused.
4. Press `e`, edit a name, Tab to Add time, enter `25m`, and save. Enter invalid
   time once: the error and draft should remain until corrected or cancelled.
5. Press `l` to reveal actions; `h/l` moves between them. Reset and Delete open
   confirmation with Cancel focused by default. Enter immediately cancels.
6. `/` filters tasks, `p` filters projects, and Ctrl+K filters commands. Arrows
   choose and Enter opens. Esc returns one level and restores selection.
7. In forms, Tab and Shift+Tab reach every control, including the bottom of a
   long settings page. In main browse mode, Tab switches bar panels as before.
8. Start tasks in two projects. The “running · all projects” view lists both;
   selecting one switches to its project. Stop it and verify the count updates.

## Settings, integrations, and appearance

1. Set a project rate/currency and confirm the live estimated amount appears.
   Switch projects and verify each retains its own rate. An invalid save keeps
   the draft. See [rate checks](manual-rates.md).
2. In Reports, change weekly/monthly schedules and save. This must preserve a
   previously selected custom template. Check previous-week/month exports.
3. Open PDF templates and appearance. Select, create, edit, preview, and refresh
   a template; save accent, paper, and logo settings. See [template checks](../TEMPLATES.md).
4. Configure Drive in Preferences & Drive. Save and sync; verify a failure
   remains visible and tracking stays usable while an upload runs.
5. Preview the wooden click, change volume, then disable it. Start/pause, typing,
   saving, switching views, and exports must all remain silent.
6. Enable Reduced motion: view slides, accent pulses, action reveals and button
   color fades become immediate. Omarchy's own outer popup fade remains native.
7. Try light/dark themes, increased font size, a small monitor, long project/task
   names, many tasks, and no tasks. Controls should remain legible and reachable;
   long labels should wrap or elide rather than overlap timers.

## Hourly semantics

- In a fresh hourly demo, pause at 59:50 and wait: no click. Resume: the remaining
  tracked seconds complete the hour, with up to 10 seconds of polling latency.
- Start another task simultaneously: two timers still advance the hourly work
  cadence at one second per second, not twice as fast.
- Add manual time or reset visible counters: neither creates an hourly sound.
- Close only the installed popup: its hour click still works. Restart the shell
  after an extended gap: missed sounds should not replay. Audio requires a running
  shell; this feature does not install a separate sound daemon.
- With the widget on two monitors, the same ledger should yield one click, not
  one per monitor. The sidecar claim is serialized under the ledger lock.

## Reproducible automated checks

```bash
make check
python tests/ui-check.py --wayland
python tests/ui-check.py --preview --hour-demo --smoke
python tests/audio-output-check.py
```

The first command runs Rust, Clippy, Typst, manifest, service and offscreen UI
checks. The second also tests the actual layer-shell popup on the current desktop.
The third checks real backend → service → completed playback at an hour boundary,
at volume zero, then exits automatically. It waits for completion rather than
exiting immediately after requesting playback. The fourth requires `pactl` and
`parec`: it records only a temporary virtual sink and verifies that three Qt
playbacks produce three non-silent bursts. Human listening is still needed to
judge the wooden click's tone and volume on the selected physical output.

## If a click is silent

Open Preferences and use **Preview wooden click**. The status now displays the
output device, app volume, and playback completion; errors remain visible even
when periodic milestone polling succeeds. The disposable preview also prints
these diagnostics and the hourly milestone to its terminal.

Check that the displayed output is the device you are listening to. The player
follows the system default when it changes. App volume and the system output's
volume/mute are separate. A completed playback confirms Qt consumed the sound;
it cannot confirm physical speakers/headphones reproduced it. The sound includes
a short silent lead-in/tail and a fuller 150 ms audible body so it is less easily
lost on short-lived output streams.
