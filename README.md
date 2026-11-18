<!-- FORK(readme): this section is the fork's; everything from "# Herdr" down is upstream's README, unchanged. -->
# Cameron's Herdr fork

This is a personal fork of [Herdr](https://github.com/herdrdev/herdr-app), the desktop app for terminals and coding agents. It tracks upstream closely: a weekly job pulls in upstream's changes, and everything below the line further down is upstream's own README, left as it is.

The fork adds a browser pane, features carried over from the archived fork of the Herdr terminal app ([cameronsjo/herdr](https://github.com/cameronsjo/herdr)), and a few fixes and extras. Nothing here goes back upstream.

The install commands in upstream's README install upstream's app, not this one. To run the fork on a Mac, install upstream's Herdr first (the fork's build switches back to it when you turn it off). Then build from source as [CLAUDE.md § Building](CLAUDE.md#building) describes (the `vendor/ghostty` submodule and Zig 0.16): `just browser-build --webkit` builds the app with the browser pane, and `bash scripts/browser-daily on` makes it the everyday app ([details](docs/browser-dev.md)).

## What the fork changes

### A web page in a pane (macOS)

- **Browser pane.** Open a real web page beside your terminals, using the Mac's own WebKit engine (a Chromium build is also there, off by default). It saves as an ordinary mod block, so a stock Herdr shows it as a mod that didn't load instead of failing to load the layout.
- **Agents can drive it, with your say-so.** A coding agent in a Herdr terminal can read and click a page in a separate test profile through [agent-browser](https://github.com/vercel-labs/agent-browser), but only after you click **Allow agent…** and confirm. **Stop agent** cuts it off. The agent always reaches the window that holds its terminal now, even after the terminal moves.
- Details: [design and security model](docs/browser-prototype.md), [building and running it](docs/browser-dev.md), [moving to upstream's browser when it ships](docs/browser-transition.md).

### Knowing which agents need you

- **Why an agent needs you.** The "Needs you" badge says why when the agent reports it: Approval, Question, Sign in or Error.
- **Flag a turn that ends with a question.** A hook or script can mark an agent that finished but is waiting on you in words, so it doesn't sink among the done ones. An agent stuck on the same screen for too long can show as Stalled (off until you turn it on). [More](docs/fork/attention.md).
- **Answer approvals from outside the pane.** The fork's `herdr-app` command gains `approval show t3`, which prints the prompt a blocked agent is showing, and `approval answer t3 approve --token …`, which answers it, from a shell, a script or another device, without pressing keys blind. [More](docs/fork/approval.md).
- **Decisions pane.** When Claude asks a batch of questions through cadence's ask-questions flow, the cards open in a Herdr pane beside its terminal instead of a browser tab. Pick options, compare, add notes, review and send.

### The agents list

- **Task line.** `hc label <terminal> task="…"` shows a short line under an agent's row saying what it is on.
- **Names.** An agent started with a name shows that name in its row.
- **Needs-you view.** A sort that shows only the agents waiting on you.
- **Grouped by project.** A sort that groups agents under a `project` label, across spaces.
- **Agents running elsewhere.** A program outside Herdr's terminals can show up in the list by writing a small status file, under "Elsewhere".

### Spaces and tabs

- **Move a tab to another space**, panes and running terminals included, and **merge two spaces** into one (`tab.move_to_space`, `space.merge`).
- **Open a folder's existing space.** The `open-folder.open:<folder>` command (run with `herdr-app do`) goes to the space already working in that folder instead of making a second one.
- **Move a repo's worktree spaces as one group** with `space-groups.up` and `space-groups.down`.
- **A named agent names its tab**, and the tab follows when the agent's name changes. A tab you renamed yourself is left alone.
- **Extra environment variables for an agent**: start it with `argv: ["env", "KEY=value", "claude"]`; no shell quoting involved.

### Command palette

- **Mods can add their own commands** to the palette by listing them in their `mod.json`.
- **One row per direction family.** "Split pane…", "Focus pane…" and "Swap pane…" open a small chooser where an arrow key picks the direction.

### Look and settings

- **Artificer theme**, dark and light, is the default for layouts that don't name their own theme.
- **Font picker lists every font.** Upstream's shows only the first 200 font families unless you search.

### Under the hood

- **Background mods.** A mod can run with nothing on screen, which is what the folder, worktree-group, tab-name and outside-agent features above use.
- **Release on merge.** Each change merged to `master` is built on GitHub's Mac runners and published as an attested prerelease. A small job on Cameron's Mac checks hourly, verifies the build came from this repo's workflow, and installs it, never quitting a running app.
- **Weekly upstream sync.** A scheduled Claude session reviews upstream's new code for supply-chain risk before running any of it, then merges it in. A clean sync merges itself once tests pass. One with conflicts or a workflow change waits for a person, and a security concern stops the sync and opens an issue. [How it works](docs/fork/weekly-sync.md).
- **Extra CI** for what upstream's doesn't test: the WebKit browser build on macOS and the fork's own mods.

Every difference from upstream, with the exact files it touches, is in the [fork ledger](docs/fork/CHANGES.md). The rules for keeping syncs easy are in [CLAUDE.md](CLAUDE.md).

---

# Herdr

Herdr is a desktop app for terminals and coding agents. Keep work in spaces, see which agents need you, and leave terminals running when you close the window.

## Install

### Linux and macOS

```sh
curl -fsSL https://releases.herdr.dev/app/install.sh | sh
```

Linux needs a desktop session and a working Vulkan driver. macOS builds are available for Apple Silicon; Intel Mac users can [build from source](#build-from-source). Linux builds are x86-64 and ARM64.

The installer adds Herdr to your apps and puts the command in `~/.local/bin`.
Add that folder to your PATH if the installer asks.
On macOS the app lives in `~/Applications/Herdr.app`.
For drag-and-drop installation on Apple Silicon, download [Herdr.dmg](https://releases.herdr.dev/app/Herdr.dmg) and drag Herdr to Applications.

On macOS (Apple Silicon), you can also install with [Homebrew](https://brew.sh/):

```sh
brew install --cask herdr-app
```

### Windows

Run in PowerShell on x64 Windows with curl and tar installed (Windows 10 1803 or newer):

```powershell
irm https://releases.herdr.dev/app/install.ps1 | iex
```

Herdr appears in the Start menu. The installer adds `%LOCALAPPDATA%\Programs\herdr-app` to your user PATH. Open a new terminal to pick it up.

The shell and PowerShell installers download and run code. Read the [shell installer](distribution/install.sh) or [PowerShell installer](distribution/install.ps1) first if you prefer.
They also install [herdr-core](https://github.com/herdrdev/herdr-core) if it is missing.
The app can help install or update a core it cannot use.

## Find your work

A **space** holds work on one machine, usually in one repo. Each space has **tabs**. Split a tab into **panes** to put terminals or mod panels beside each other.

An unnamed space follows its first terminal's folder. Name a space or tab when you want a label that stays.
New tabs start in the focused terminal's folder.

The agents sidebar brings agents from your spaces into one list, including workers started by another agent.
Agents that report status show as:

- **Working:** still busy.
- **Blocked:** waiting for you.
- **Done:** finished while you were looking elsewhere.
- **Idle:** not working, or a finished agent you have looked at.

Click an agent to see it. A worker without a pane opens in **peek**, over your tab. Keep it as a tab if you want it beside your other work.
Ctrl-click a live terminal address in terminal text to peek at it; use Cmd-click on macOS.
An address such as `t7` means a terminal on the same machine as the terminal showing it. `vps/t7` names one on another machine.

The window arranges terminals. The core runs them.
Closing the window leaves them running; closing a pane or tab ends its terminals.
After a reboot or core crash, kept panes get fresh shells in their last folders, not their old programs.

## Work on another machine

Add an SSH machine from Settings → Machines or the sidebar's Add machine action.
Pick a host from your SSH config or type how SSH reaches it.
Spaces on that machine run their terminals there, not on your computer.

Each machine keeps its spaces, tabs and splits in its core. Connected screens share that structure and the live terminals, but keep their own focus and zoom.
Remote work keeps running when your local window closes or your laptop sleeps, as long as the remote machine and core stay up.

## Make it yours

Open the command palette to find an action by name, or open Settings to change fonts, colors, sounds and other preferences.
Settings > Look also adjusts terminal line height, cell width and scroll speed. All default to 100%; layouts can override them with `terminal_line_height` (0.8–2), `terminal_cell_width` (0.8–2) and `terminal_scroll_speed` (0.1–5).
The built-in layouts include Herdr, Terminals and Herdr 98.
A **layout** arranges the window. **Mods** are small JavaScript programs that draw its panels, menus and dialogs.

Duplicate a shipped layout or mod before changing its structure or code. Your copies stay yours; Herdr updates the shipped ones on start.
Save a personal layout, mod or key file and the window reloads it.
A broken edit keeps the last good version and reports the error.

Ctrl+Shift+P shows every command and its key (Cmd+Shift+P on macOS). Change any key in `keys.json`; see [the keys section](defaults/AGENTS.md#keys-keysjson) of the mod guide for the format, including JSON objects for named, typed command arguments.

On macOS, Cmd+A selects the terminal's whole screen and scrollback, ready for Cmd+C.
In vim and other alternate-screen apps, it selects only that screen.
Linux and Windows keep Ctrl+A for the shell; bind `terminal.select_all` in `keys.json` if desired.

[Layouts and mods](docs/mods.md) · [Commands](docs/commands.md)

## Updates and privacy

Installed releases check for updates in the background and download a new app beside the running one. It takes over when you quit and reopen, or choose the update action. Your terminals keep running in the core.
Settings → Updates shows what is available. What's New opens after an update; you can open it again from the command palette.
Builds from source do not update themselves.

Usage telemetry is on by default in releases. Turn it off in Settings → Privacy.
Mods are trusted code, not sandboxed extensions. Read [security and privacy](docs/security.md) before installing someone else's mod.

## For agents

```sh
herdr-app --skill
```

This prints the agent guide for driving the window. The herdr-core skill points agents here.
Use herdr-core for running and coordinating programs; use the app to change what you see.

```sh
herdr-app ls
herdr-app status
herdr-app commands
```

The first two inspect the open window. The command list also works without one.
See the [command reference](docs/commands.md) for arguments.

## Build from source

For bugs, mod API requests and code contributions, see [Contributing](CONTRIBUTING.md).

Clone this repo with its submodules. Use Rust 1.95.0 and Zig 0.16.0, the versions used in [CI](.github/workflows/ci.yml).
Linux needs the development packages listed in that workflow.

```sh
git clone --recurse-submodules https://github.com/herdrdev/herdr-app.git
cd herdr-app
just build
just test
just install
```

Install [just](https://github.com/casey/just) to use these recipes.
The platform installer builds a release and adds the app to your desktop.
On Linux it links to this checkout, so keep the checkout in place.
Quit and reopen Herdr to use the new build.

Documentation checks also need [Bun](https://bun.sh): `just docs-check` checks generated pages and CLI examples; `just docs-generate` refreshes the reference pages.

To compare drawing changes on Windows, Linux or macOS, run the same fixed workloads against each build:

```sh
python scripts/bench.py --bin <app executable> --set spinners --seconds 20
```

The runner requires `hc` on PATH and uses the Python executable running the script. It prints a unique artifact folder with the run settings, counters and results, then stops its private app, terminals and core. Every set uses Python fixture programs; the default set’s TUI workload replaces historical `btop` measurements. `--only spinners` measures just the quiet native spinner scenario. `--set titles` runs eight agents whose titles spin after a prefix (`π ⠹ task`) on the main layout. Keep the window visible and the display awake, and compare the same window activity and display settings; inactive windows may draw at a lower rate. Windows records visibility, foreground activity and display Hz at each measurement boundary; other platforms mark these observations unavailable. Visibility does not establish occlusion. Draws are window render calls. Render-to-paint elapsed time stops before scene finalization, backend drawing, uploads and presentation; it is separate from OS process CPU time. Neither measures physical presentation or the FPS HUD’s derived MAX FPS.

`python scripts/bench_compare.py ghostty --root <artifact folder>` reuses a run’s fake agent for the macOS comparison. Benchmark checks: `python -m unittest discover -s scripts -p test_bench.py`.
