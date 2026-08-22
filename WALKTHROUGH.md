<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Trynta — a spoken walkthrough

A narration script for a screen recording of the app as it stands today. Read it top to bottom
and you will have covered the stack, every screen, every setting, and the things that are
deliberately missing.

Each section has an **On screen** cue — what to be showing — and then the words. The paragraphs
are short on purpose; they are meant to be said out loud, not read. Say them in your own voice,
skip whatever does not fit the pace, but don't upgrade the claims: everything here is true of the
current build, and a few sentences exist specifically to stop the demo overselling itself.

**Recording notes.** Use a fresh vault so the first-run flow is real. The app opens in light
theme by default; switch to dark on camera once, in Settings → Appearance, because the swap is
instant and it looks good. Every credential you show should be obviously synthetic — the shipped
screenshots use `trynta.example`, a reserved domain that can never resolve.

---

## 1. What this is

**On screen:** the finished vault, one item selected.

This is Trynta. It's a password manager for the desktop that keeps everything on your own
machine, encrypted, with no account to sign up for and no server behind it.

There is no cloud. There is nothing to log into. You create a vault, you pick a master password,
and that's the whole setup.

I'll show you what it does, then I'll show you what it's built out of, and at the end I'll be
straight with you about what it can't do yet — because there's a real list.

---

## 2. The stack

**On screen:** the repository, or a simple slide. Don't linger.

The shell is Tauri v2. That matters mostly for what it isn't — it isn't Electron, so there's no
bundled browser and no Node runtime sitting next to your secrets.

The entire backend is Rust. Every key, every encryption and decryption, every byte of storage. The
interface is React and TypeScript, and the interface never holds a key. It can't. There's exactly
one narrow path that hands a plaintext to the screen, and I'll show you where.

Storage is SQLite, with the encryption at the field level. The database file is not the security
boundary — the ciphertext inside it is. If someone copies your vault file, what they have is a
list of encrypted blobs.

The styling is Tailwind, on top of a token layer, which is why themes can be swapped live and why
you'll be able to write your own.

---

## 3. First run

**On screen:** the "Create your vault" screen.

Fresh install. It asks for one thing: a master password.

This is the only password you have to remember, and it's the only one Trynta can't help you with.
It isn't stored anywhere, it isn't sent anywhere, and there is no reset link. If you lose it, the
vault is gone. That's not a limitation to work around, it's the design.

Behind this button, your password goes through Argon2id — a key derivation function built to be
slow and memory-hungry, so that guessing at it in bulk is expensive.

And it calibrates itself to your machine. It measures how long a derivation takes here and picks
the cost so that unlocking takes about seven tenths of a second on _your_ hardware. A faster
computer gets a harder setting, automatically. There's a hard floor it will never go below, so a
slow machine can't calibrate its way down to something weak.

That cost is written into the vault header, so the file unlocks the same way on any machine you
move it to.

---

## 4. Unlock, and what locking actually means

**On screen:** lock with Ctrl+L, then unlock.

Control+L locks it. Watch — that's not a screen overlay. Locking wipes the keys out of memory,
tears down every decrypted cache, and clears the clipboard if what's on it is something Trynta
put there.

Unlocking runs the derivation again. Get it wrong and there's a backoff that grows with each
failure — and it survives restarting the app, so you can't reset it by closing the window.

What it deliberately does _not_ do is wipe your vault after a number of failed attempts. That
sounds tough, but anyone who has your file can trigger it, and then it's just a way to destroy
your data.

---

## 5. Windows Hello

**On screen:** Settings → Security → the biometrics toggle, then lock and unlock with Hello.

This is the one I like. Turn on "Unlock with biometrics" and it asks for your master password
once — right there, before it enables anything.

Here's why. Windows Hello creates a key pair that the operating system keeps behind your face or
your fingerprint, in the TPM chip where the machine has one. Trynta never sees that private key.
It asks the OS to sign a fixed challenge, which is what makes the prompt appear, and derives a
wrapping key from the signature that comes back.

That wrapping key protects your master password, which is handed to Windows' own secure storage —
DPAPI and Credential Manager — and is only released after Hello signs again.

So it stores your password, not a derived key. That's deliberate. The slow derivation is the thing
that makes a stolen vault expensive; storing a key would skip it. This way, somebody who defeats
Hello gets exactly what you have and nothing more, and every check on the normal unlock path still
runs.

Two more things. If you change your Hello enrolment, Windows destroys that credential, and Trynta
falls back to the master password — it doesn't try to be cleverer than the OS about that. And your
master password is required at least once every fourteen days regardless, so a fingerprint can
never carry the vault forever.

It's off by default, and it's the only feature here I'd flag as less tested than the rest — the
signing path isn't covered by the automated gate.

---

## 6. The vault

**On screen:** the sidebar, then the item list.

Down the left: your vaults at the top, then the library, then the tools.

You can have as many vaults as you like — Personal, Work, whatever — each one renameable and each
one with its own colour. The number next to each is a live count.

Underneath, the whole library at once: All items, then Logins, Secure notes, Cards, Identities,
and Favourites. Four item types, and everything is fully typed rather than one generic "entry"
with optional fields.

The list is keyboard-driven. Arrow keys move, Return opens, and Control+C copies the password of
whatever is highlighted — without ever showing it.

Those coloured dots are risk flags, and they'll make sense in a minute when we get to the security
report.

The service logos are bundled inside the application. They are not fetched. That's not a
performance decision — asking a server for a favicon for every entry would broadcast, in the
clear, a list of every service you hold an account with.

---

## 7. An item, and the two ways a secret moves

**On screen:** a login with a password and a one-time code.

Here's a login. Title, username, password, website, notes, a one-time code.

Two things can happen to that password, and they're different.

**Copy.** The password is decrypted in Rust and written straight to the Windows clipboard. It never
enters the interface — the part of the app that draws this screen never sees the value. And it
clears itself after thirty seconds by default, but only if what's on the clipboard is still the
thing Trynta put there. If you copied something else in between, it leaves your clipboard alone.

**Reveal.** This is the single exception: one field, one item, on an explicit click. It's limited
to twenty reveals a minute, and it disappears the moment you click away, navigate, or lock. There
is a setting to require your master password every time you do it, which is off by default —
because the rate limit already asks for re-authentication, and typing a master password all day
is its own risk.

---

## 8. One-time codes

**On screen:** the TOTP field counting down.

That's a real two-factor code, generated locally. Standard TOTP — the same RFC that every
authenticator app implements.

Paste an `otpauth://` link, or just the setup key on its own, and it works out the rest. It
handles SHA-1, SHA-256 and SHA-512, and both six and eight digit codes, because plenty of services
use something other than the default and most tools quietly assume nobody does.

The ring is the countdown. Click the code to copy it.

---

## 9. Search and the command palette

**On screen:** Ctrl+K, type a few characters.

Control+K anywhere. This is fuzzy search over your whole vault plus the actions — generate a
password, run the security report, open settings, show all items.

The index is built in memory at unlock, from decrypted metadata, and thrown away when you lock.
On a vault of five thousand items the ninety-fifth percentile search is under sixteen milliseconds
— that's a measured number from the acceptance gate, not an estimate.

---

## 10. The generator

**On screen:** the generator, switching between the two modes.

Two modes. Passwords and passphrases.

Passwords: set the length, choose uppercase, digits, symbols, and optionally avoid the characters
that look like each other — the ones, the lowercase Ls, the zeros and Os.

Passphrases use the EFF long word list — seven thousand seven hundred and seventy-six words, which
ships inside the application and is verified against its published hash at build time. Words are
separated by spaces, because a hyphen makes the boundaries ambiguous when you read it aloud or
retype it, and the separator adds no strength either way.

Randomness comes from the operating system, with rejection sampling — which is the boring detail
that keeps every character equally likely instead of subtly favouring the start of the alphabet.

And this number is the point. That's the actual entropy in bits — the real measure of how hard
this is to guess. Not a coloured bar that turns green when you add an exclamation mark.

The history below keeps what you generated — but only the kind, the strength, and when. It never
stores the value. A list of your recent passwords sitting in a panel would be a strange thing for
a password manager to keep.

---

## 11. The security report

**On screen:** the security report, then click through to a flagged item.

This is the health check. A score at the top, and the arithmetic behind it right underneath —
you can see exactly why it says what it says.

Five figures. Breached. Weak. Reused. Accounts that offer two-factor and don't have it set up. And
"Not checked".

That last one exists on purpose. An unchecked password is an unknown, and this report will never
show you an unknown as safe. Those are different things and folding them together is the lie the
card is there to prevent.

Underneath, every finding is a row you can click straight through to.

Now — the breach check. It's off by default. When you turn it on, here is exactly what leaves your
machine: the first five characters of a SHA-1 hash. Not your password. Not the site. Five
characters of a hash, sent to Have I Been Pwned, which returns a bucket of hundreds of hashes, and
the matching happens here on your machine.

It runs at most once a day. And opening this report sends nothing at all — the report reads a local
cache. Refreshing the cache is a separate thing you press.

---

## 12. Backup and restore

**On screen:** Settings → Backup and restore, export a file.

One encrypted file. You pick where it goes.

The important detail: the backup has its own passphrase, chosen for the file, separate from your
master password. So you can hand a backup to a safe deposit box without handing over the key to
your vault — and you can change your master password without invalidating the archive.

Keep a record of both, though. Neither one is recoverable.

The archive is signed, so a restore refuses a file that's been tampered with, and it carries its
own derivation cost so it stays expensive to attack on its own terms.

---

## 13. Themes

**On screen:** Settings → Appearance, switch light → dark → system.

Light, dark, or follow the system. It opens on light.

Watch the swap — no reload, no flash. Every colour in the app resolves through a token layer, so
changing theme is a change of data rather than a change of stylesheet.

Which is also why you can import your own. A theme is a JSON file of token values. It's validated
in Rust before a single value is applied — it cannot make a network request, and it cannot contain
a `url()`, so a theme can't be a way to smuggle a tracker into a password manager.

---

## 14. The rest of Settings

**On screen:** scroll the settings pane top to bottom, pausing on each row.

Everything in one place, so let me go through it properly.

**Security.** Biometric unlock, which we covered. Clipboard clearing — on by default at thirty
seconds, with five seconds up to five minutes if you want it. "Require the master password to
reveal" — off by default, on if you want the extra friction. "Watch for breaches" — off by
default, and it's the switch that turns on that one outbound request. And "Hide from screen
capture", which excludes the window from screenshots and screen sharing. That's off by default
too, and honestly: turning it on breaks your own screenshots and your own screen share when you
need help.

**Appearance.** Theme, and imported themes.

**Autofill and import.** Not in this version — and notice it says exactly that, rather than
offering a switch that does nothing. When autofill does arrive, it will match on the registrable
domain through the Public Suffix List, never on a substring. That difference is the difference
between a password manager and something that hands your credentials to a lookalike site.

**Vaults.** Create, rename, recolour, remove.

**Backup and restore**, which we just did.

**Updates.** It checks a signed manifest, at most once every twenty-four hours, on launch — and it
works while the vault is locked, because a patch channel you can only reach after unlocking is not
much of a patch channel. The signature verification is done by Tauri's updater against a key
compiled into the binary; there's no code path here that can skip it. You can turn the whole thing
off.

**Help.** Replays the introduction tour.

---

## 15. The tour

**On screen:** press the tour button, walk through the four cards.

Speaking of which — first launch gives you this.

One card on the lock screen explaining what a master password actually is, and then four inside
the app: the list, the generator, the security report, and backup. The card travels between them
rather than popping in and out, and nothing behind it is blocked — you can keep using the app
while it's up.

It runs once, and it's here whenever you want it again.

---

## 16. What leaves your machine

**On screen:** stay on Settings, or a plain slide.

Let me be precise about this, because it's the whole point of the product.

Two network requests exist in the entire application. Two.

One: five characters of a hash to Have I Been Pwned, only if you switch breach checking on.

Two: the signed update manifest, only if you leave update checks on.

That's it. No sign-up. No sync. No telemetry. No analytics. No crash reporter. The app never probes
the sites you have accounts with — not for icons, not for anything. There's a check in the build
that fails if a third outbound call site ever appears, so this isn't a promise, it's enforced.

---

## 17. What it can't do yet

**On screen:** the README, or a plain slide. Say this at normal pace — don't rush it.

Now the honest part.

This is pre-1.0 and it has never been security-audited. No third party has reviewed the
cryptography, the storage format or the platform code. Don't put credentials you actually rely on
in it.

Windows is the only verified platform. There's macOS code in the repository, and it compiled and
passed the full gate once, in August — and nothing macOS has been compiled since. There is no Mac
build.

Sharing is not built. Multi-owner credentials — two people genuinely co-owning one login — are the
reason this project exists, and they don't exist yet. The key material is generated and reserved
for it; nothing uses it.

No sync. No autofill and no browser extension. No import from another manager.

I'd rather show you that list than have you find it.

---

## 18. Where it's going, and where to find it

**On screen:** the GitHub repository.

The whole thing is open source under the AGPL. The source is the whole product — there's no closed
component, no paid tier, and no licensing code in the repository at all.

Security issues go to the private advisory form on GitHub, not a public issue. Everything else —
bugs, ideas, questions — is welcome as an issue.

Next up is the thing it was built for: real multi-owner sharing, where two people co-own a
credential and neither one depends on the other being online.

Thanks for watching.

---

## Appendix — numbers, if you get asked

Facts you might want on hand in the comments or a follow-up. Every one of these is checkable in
the repository.

| Claim             | Detail                                                                                                                                                                                                  |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Key derivation    | Argon2id v0x13. Default cost m = 65,536 KiB, t = 3, p = 4; calibrated per machine toward ~700 ms; hard floor m = 19,456 KiB, t = 2. Cost stored in the vault header.                                    |
| Field encryption  | XChaCha20-Poly1305, one envelope per field. Subkeys via HKDF-SHA256.                                                                                                                                    |
| Signatures        | Ed25519, on the vault manifest and on backup archives.                                                                                                                                                  |
| Reserved          | An X25519 keypair is generated and stored for sharing. No key agreement runs anywhere.                                                                                                                  |
| Length hiding     | Plaintexts are padded to a 256-byte boundary (ISO/IEC 7816-4) so a ciphertext length can't tell a PIN from a note.                                                                                      |
| Key hierarchy     | Per-item keys, wrapped by per-vault keys, wrapped by keys derived from the master password.                                                                                                             |
| Memory            | All key and plaintext buffers are zeroized. The three 32-byte session keys are pinned with `VirtualLock`. The Argon2 buffer is a documented exception — it is far larger than the lockable working set. |
| Reveal limit      | Twenty per rolling minute.                                                                                                                                                                              |
| Clipboard         | Default 30 s; options 5, 15, 30, 60, 120, 300 s. Cleared only if the clipboard still holds what Trynta put there.                                                                                       |
| Biometric re-auth | Master password required at least every 14 days.                                                                                                                                                        |
| Breach check      | HIBP range API, 5-character SHA-1 prefix, `Add-Padding: true`, at most once per 24 h, opt-in.                                                                                                           |
| Update check      | At most once per 24 h on launch, works while locked, minisign-verified by `tauri-plugin-updater`, opt-out.                                                                                              |
| Search            | p95 under 16 ms at 5,000 items; the last gate run measured 4.28 ms (p50 1.70 ms).                                                                                                                       |
| Passphrases       | EFF long list, 7,776 words, vendored and hash-verified. Space separator.                                                                                                                                |
| Entropy bands     | Weak below 40 bits, Fair from 40, Strong from 65, Excellent from 90.                                                                                                                                    |
| Brand icons       | 3,772 marks bundled in the binary. Never fetched.                                                                                                                                                       |
| Licence           | AGPL-3.0-or-later. Copyright © 2026 Ahmed Amin.                                                                                                                                                         |
