# Stowey® Voice: rules for every session

Stowey® Voice is a macOS dictation app. It is a thin overlay on the open source app Handy (cjpais/Handy, MIT). This file loads at the start of every Claude Code session in this repository. The long record is `.claude/handover.md`. Read it before you change anything.

Upstream's own instructions load too: `CLAUDE.md` points at `AGENTS.md`. They describe Handy and are good for commands and code layout. Where they and this file disagree, this file wins.

## The overlay rule

- Never commit a change to a file that came from Handy. Stowey owns `stowey/`, `.github/workflows/stowey-voice.yml`, `README.md` and `.claude/`. Nothing else.
- Every change to the app is a patch in `stowey/apply.py`, or a file in `stowey/` that the script copies in. Every patch asserts its anchor, so a change upstream breaks the build loudly.
- The reason: taking a new Handy release must stay free of conflicts. Only `README.md` may ever conflict.
- `stowey/apply.py` rewrites about 70 upstream files in place and refuses to run twice. Never run it in the checkout you commit from. Build in a second worktree. The handover has the steps.

## What starts a build

- A push to `main` that touches `stowey/` or the workflow file builds, tests and publishes a release. The download links the team uses serve it at once.
- A push to a branch named `try/` plus anything builds and tests the same way and releases nothing.
- A run takes about 20 minutes with a warm cache and uses up a release number.
- `.claude/` and `README.md` start nothing.

## Releasing

- A release reaches the whole team. Ship one only for a change the person you work for asked for, and only once it is proven: the tests pass, and a local build or a `try/` build shows it working.
- Never release something nobody asked for. Never delete a release, a tag or a branch without a clear yes.

## This repository is public

- No names of colleagues, customers or suppliers anywhere: code, tests, comments, commit messages, this folder. The only names in the public code are "Stowey" and "Chemaly". Tests use made up names.
- No secrets, no keys, no private word lists, nothing from anyone's dictations.

## The people you work for

- They run a company. They are not developers. Do the work yourself: never hand over commands, code or files to run.
- Report briefly, in plain lists: what you did, what it changes for them, what they have to do, if anything.
- No dashes of any kind in text written for people. Rewrite the sentence. Codes, file names and URLs keep their characters. No exclamation marks. British English. Sentence case in headings.
- The name is Stowey® Voice wherever a person reads it. Never with a trademark sign in place of the registered mark.
- Say what is verified and what is not. Never guess a figure. A log line or a photograph beats a claim.
- Only a person does these: type a password or a code, approve a permission on the Mac, accept an agreement, pay.
- Stowey files this work under Flow 52 ID Digital Infrastructure. Name that code when you report.

## After every change

- Update `.claude/handover.md` in the same commit: state, decisions, verified and not verified, open items.
- Upstream's `.gitignore` ignores `.claude/`. The files already tracked there commit as usual. A new file needs `git add -f`.
