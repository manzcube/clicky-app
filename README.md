<div align="center">
<img src="src-tauri/icons/128x128.png" width="88" alt="clicky" />

# clicky

**Highlight text anywhere. A small bead follows your cursor and offers to
translate it, reply to it, rewrite it, or explain it.**

Runs entirely on your own machine through [Ollama](https://ollama.com).
Nothing you highlight is sent anywhere.

</div>

---

## What it does

Select some text in any app. The panel opens right there, with what you
selected already loaded:

| | |
|---|---|
| **Translate** | into whichever language you last picked |
| **Reply** | drafts an answer to a message or email, matching its tone |
| **Polish** | rewrites it more clearly, keeping your voice |
| **Explain** | four sentences on what it means |

Or type your own question about it. **Replace selection** pastes the result
straight back over the text you had highlighted.

It opens without taking your keyboard, so it can't interrupt you mid-sentence
— click a button, or click the box to type. Ignore it and it closes itself.

Too eager? Set `auto_open` to false and it waits for you to click the bead
instead. Turn off *Follow cursor* in the menu bar and there's no bead at all —
just **⌘⇧Space** (Ctrl+Shift+Space on Windows and Linux) when you want it.

---

## Install

### Download a build

Grab the file for your machine from
[Releases](https://github.com/YOURNAME/clicky/releases):

| Platform | File |
|---|---|
| macOS, Apple Silicon | `clicky_x.y.z_aarch64.dmg` |
| macOS, Intel | `clicky_x.y.z_x64.dmg` |
| Windows | `clicky_x.y.z_x64-setup.exe` |
| Linux | `clicky_x.y.z_amd64.AppImage` |

macOS will say the app is from an unidentified developer unless you've set up
signing. Right-click the app → **Open** → **Open** clears it once and for all.

### Then install a model

clicky needs Ollama running with one model pulled.

```bash
# macOS / Linux
curl -fsSL https://ollama.com/install.sh | sh
# Windows: download from ollama.com

ollama pull gemma3:4b
```

`gemma3:4b` is the default — about 3GB, good at translation, fast enough to
feel instant. Any model you've pulled works; set it in the settings file.

**Do this too.** It keeps the model in RAM between uses, which is the
difference between clicky feeling instant and feeling broken:

```bash
# macOS / Linux — add to ~/.zshrc or ~/.bashrc
export OLLAMA_KEEP_ALIVE=30m
# Windows PowerShell
setx OLLAMA_KEEP_ALIVE 30m
```

### First run

clicky lives in your menu bar / system tray, not your dock.

**macOS will ask for Accessibility permission, and clicky does nothing useful
without it.** System Settings → Privacy & Security → Accessibility → turn on
clicky. It needs this to read what you've highlighted in other apps. If you
skip it, every selection comes back empty.

---

## Settings

Menu bar icon → **Edit settings…**, or edit the file directly:

- macOS `~/Library/Application Support/foo.clicky.desktop/settings.json`
- Windows `%APPDATA%\foo.clicky.desktop\settings.json`
- Linux `~/.config/foo.clicky.desktop/settings.json`

```jsonc
{
  "model": "gemma3:4b",
  "host": "http://127.0.0.1:11434",

  "follow_cursor": true,      // the bead trails your mouse
  "watch_selection": true,    // notice when you highlight something
  "auto_open": true,          // open the panel straight away, no click on the bead
  "auto_open_focus": false,   // give it your keyboard too (off: it won't interrupt typing)
  "auto_close_ms": 14000,     // an auto-opened panel you ignore closes itself
  "offset_x": 22,             // where the bead sits relative to the cursor
  "offset_y": 22,
  "follow_easing": 0.22,      // 1.0 = glued to the cursor, 0.1 = drifts behind

  "target_language": "English",
  "hotkey": ""                // empty = Cmd/Ctrl+Shift+Space
}
```

Restart clicky after editing. Or skip the file entirely for a quick test:

```bash
CLICKY_MODEL=qwen3:4b clicky
```

---

## Build it yourself

```bash
git clone https://github.com/YOURNAME/clicky.git
cd clicky
cargo install tauri-cli --version "^2"    # once

cd src-tauri && cargo tauri dev           # run it
cargo tauri build                         # installer in target/release/bundle/
```

There's no npm and no bundler. The frontend is two HTML files that Tauri serves
as they are. **Open `src/index.html` in a browser** to work on the panel's
design — it detects there's no app around it and fakes the model with canned
responses.

Linux needs some system packages first:

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf libxdo-dev libx11-dev libxi-dev libxtst-dev
```

### Shipping a release

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions builds macOS (both chips), Windows and Linux and attaches the
installers to a draft release. Open Releases, write the notes, publish.

---

## How it's put together

```
src/index.html            the panel — UI and the prompt for every action
src/bead.html             the thing that follows your cursor
src-tauri/src/main.rs     windows, menu bar, hotkey, commands
src-tauri/src/watcher.rs  cursor following + spotting a selection gesture
src-tauri/src/selection.rs  reading highlighted text out of other apps
src-tauri/src/model.rs    the only file that knows what a model is
src-tauri/src/settings.rs
```

**Adding an action** is two edits: a `<button class="chip" data-mode="yours">`
in `index.html` and a matching entry in the `MODES` object below it. Nothing
else in the app knows modes exist.

**Using something other than Ollama** is one edit: the body of `stream()` in
`model.rs`. The frontend only knows about the `ask_model` command and the
`token` / `done` / `error` events. Anything that emits those works — including
a cloud API, for people whose laptop can't hold a model in RAM.

### How it notices a selection

There's no cross-platform "the user selected text" event. The options are
polling accessibility APIs (slow, and unimplemented in plenty of apps) or
recognising the gesture. clicky recognises the gesture: a mouse drag over ~9
pixels, or a double click. Only then does it send a copy keystroke and read the
clipboard, putting your previous clipboard contents back afterwards.

So clicky is *not* watching your screen continuously. It reads exactly once,
immediately after you highlight something. It's also why the Accessibility
permission is required, and why it can't read password fields or some
sandboxed apps.

---

## Known rough edges

This is version 0.1. Honestly:

- **Following the cursor is divisive.** Some people love it, some find it an
  eye-floater within a day. The bead ignores mouse clicks while idle so it
  never eats a click, but it's still a thing moving in your peripheral vision.
  *Follow cursor* in the menu bar turns it off.
- **The panel steals focus on macOS.** It should be a non-activating `NSPanel`
  so summoning it doesn't deactivate the app you're in. See
  [`tauri-nspanel`](https://github.com/ahkohd/tauri-nspanel). This is the main
  thing between 0.1 and feeling like a real product.
- **Cold start.** The first request after a long gap waits for Ollama to load
  the model. `OLLAMA_KEEP_ALIVE` above mostly solves it; switching models
  re-triggers it every time.
- **Selection detection misfires** on drag-and-drop and on some canvas apps,
  where a drag isn't a text selection. It offers, finds nothing, goes quiet.
- **Small models translate well and reason badly.** Keep the actions narrow —
  that's what they're good at.

---

## Roadmap

- Non-activating panel on macOS
- Remember target language per app — English in Slack, Spanish in Mail
- Stream the result straight into the app you're in, no Replace click
- Custom actions defined in the settings file
- Auto-update via Tauri's updater

---

MIT. Do what you like with it.
