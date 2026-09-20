# clicky — project context

A macOS/Windows/Linux menu-bar utility. A small bead follows the cursor; when
the user highlights text anywhere, a panel opens beside it offering to
translate, reply to, polish or explain it. All inference runs locally through
Ollama — nothing leaves the machine, which is the product's main claim.

Tauri v2 (Rust) + two plain HTML files. No npm, no bundler, no framework.

---

## Status

Working on the author's Mac as of this handoff: builds, launches, follows the
cursor, detects selections, auto-opens the panel, streams from Ollama.

**Never compiled by the author of the code.** It was written without a Rust
toolchain available and debugged remotely through the user. Treat the Rust as
plausible but unverified — particularly `enigo` key simulation and any Tauri v2
API whose signature may have shifted. Run `cargo clippy` early.

Default model is `gemma3:4b`. `OLLAMA_KEEP_ALIVE=30m` must be set or the app
feels broken rather than slow.

---

## Layout

```
src/index.html            the panel: all UI, and the prompt for every action
src/bead.html             the cursor-following bead
src-tauri/src/main.rs     windows, tray menu, hotkey, Tauri commands
src-tauri/src/watcher.rs  90Hz loop: cursor following + selection gesture detection
src-tauri/src/selection.rs  reads highlighted text out of other apps
src-tauri/src/model.rs    the only file that knows what an LLM is
src-tauri/src/settings.rs JSON config in the OS config dir
.github/workflows/release.yml   tag push → installers for all platforms
```

### Two load-bearing conventions

**Adding an action is two edits, both in `index.html`:** a
`<button class="chip" data-mode="x">` and a matching entry in the `MODES`
object below it. Nothing in Rust knows modes exist. Don't break this.

**The model layer is one function.** `model.rs::stream()` is the only thing
that speaks to Ollama. The frontend knows only the `ask_model` command and the
`token` / `done` / `error` events. Swapping in a cloud API or llama.cpp should
touch no other file.

---

## Hard-won details — do not undo these

**Coordinate systems.** Mouse *position* comes from `app.cursor_position()`
(Tauri, physical pixels). Mouse *buttons* come from `device_query`. They were
mixed once and the bead drifted further from the cursor the further right you
moved on a Retina display. Settings offsets are in logical points and get
multiplied by `scale_factor()`.

**The bead freezes while offering.** `bead_clickthrough(false)` also sets
`Watcher.frozen`. A window that moves between mousedown and mouseup swallows
the click — the bead was literally running away from the user's cursor.

**The bead is click-through while idle,** so it can never eat a click meant for
the app underneath. It only becomes clickable when it has something to offer.

**The panel ignores blur for its first 600ms** (`ShownAt`). macOS reports a
blur during the show-and-focus sequence, and the panel was hiding itself in the
same instant it appeared.

**The panel auto-opens unfocused** (`auto_open_focus: false`). Focusing on every
selection hijacks the user's typing. The cost is that clicking away can't
dismiss a window that never had focus, hence the `auto_close_ms` timer, which
any pointerdown/keydown/wheel/focusin cancels.

**Both windows set `set_visible_on_all_workspaces(true)`** or they vanish when
the user switches Spaces. The app is `ActivationPolicy::Accessory` — no dock
icon, no Cmd-Tab, doesn't deactivate the frontmost app.

**Clicky cannot show over another app's true (green-button) fullscreen Space —
don't try again.** Tried `NSWindowCollectionBehaviorFullScreenAuxiliary` plus
an elevated window level (the Electron-recommended recipe for "always on top
over fullscreen"). Result was worse than doing nothing: instead of silently
failing to appear, it forced macOS to kick the other app out of fullscreen
entirely. Confirmed via Apple's own developer forums that this is a deliberate
platform restriction — only genuine menu-bar extras are exempted from Spaces
isolation, no third-party window can overlay another app's fullscreen Space,
public API or private. If this comes up again, the answer is to tell the user
to use a maximized (non-native-fullscreen) window instead — that's a normal
Space and already works via `set_visible_on_all_workspaces`.

**Selection capture is clipboard-based** (`selection.rs`): send ⌘C, read
clipboard, restore previous contents, using a sentinel value to distinguish
"nothing selected" from "selected the same text already on the clipboard". The
accessibility APIs that would do this properly are missing or broken in too
many apps. Capture must run on the main thread on macOS — `watcher.rs` uses
`run_on_main_thread` and polls a slot for the result.

**Selection detection is gesture-based,** not API-based: a drag over ~9 logical
points held 60ms–12s, or a double click. Only then does it pay for a clipboard
round-trip. This is why clicky can honestly say it isn't reading the screen
continuously.

---

## macOS development reality

`cargo tauri dev` is close to useless here. It runs a loose binary with no
identity, so macOS attributes the Accessibility request to the *terminal* that
launched it. Always test the bundled app:

```bash
cd src-tauri
cargo tauri build
rm -rf /Applications/clicky.app
cp -R target/release/bundle/macos/clicky.app /Applications/
open /Applications/clicky.app
```

Accessibility permission is mandatory — without it every selection returns
empty. It does **not** apply to an already-running process: quit and relaunch
after granting.

**Unfinished as of this handoff.** Unsigned rebuilds get a new identity each
time, so macOS keeps re-asking for Accessibility and leaves stale entries in
the list. The fix in progress: create a self-signed **Code Signing**
certificate in Keychain Access named `clicky dev`, then add
`"signingIdentity": "clicky dev"` to `bundle.macOS` in `tauri.conf.json`. Verify
this actually makes the permission stick across rebuilds. Clearing a stale
state needs `tccutil reset Accessibility foo.clicky.desktop` plus removing the
row with the − button in System Settings.

---

## Known rough edges

- Selection detection misfires on drag-and-drop, canvas apps and spreadsheet
  cell drags. It opens, finds nothing, closes itself — but it's noise.
- Cold start after Ollama unloads the model is multi-second and reads as broken.
- Small models translate and rewrite well, reason badly. Keep actions narrow.
- Cursor-following is divisive; some users find it an eye-floater. Already
  toggleable from the tray.
- `tauri-nspanel` for a true non-activating panel was considered and skipped.
  `ActivationPolicy::Accessory` covers most of it; revisit if focus still
  misbehaves.

## Next up

1. Confirm the self-signed certificate fixes the permission churn.
2. Tighten gesture detection to cut false positives.
3. Remember target language per frontmost app (English in Slack, Spanish in
   Mail) — the most-wanted feature by some distance.
4. Stream results straight into the frontmost app, skipping Replace.
5. Custom actions defined in `settings.json` rather than hardcoded in HTML.
6. Tauri updater, so shipped builds can self-update.

## Working agreement

The user builds and runs; you can't. Ask for actual error output rather than
guessing. Prefer small diffs over regenerated files — the user tracks this in
git and reviews before building. Every rebuild costs them a minute plus a
possible permission reset, so batch changes rather than trickling them.
