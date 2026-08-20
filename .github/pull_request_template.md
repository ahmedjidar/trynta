## What this changes

<!-- One paragraph. What does it do, and why is it needed? -->

## What you deliberately left out

<!-- Scope you chose not to cover, and why. "Nothing" is a valid answer. -->

## What you are least confident about

<!-- Genuinely useful. Nobody thinks less of you for filling this in honestly. -->

## What you ran

<!-- Tick what you actually ran, not what you believe would pass. -->

- [ ] `pnpm test`
- [ ] `cargo test --workspace`
- [ ] `pnpm lint && pnpm typecheck`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic`
- [ ] `cargo fmt --all --check`
- [ ] `node scripts/verify-v1.mjs --run 1`
- [ ] `node scripts/verify-v1.mjs --run 2`
- [ ] `node scripts/verify-v1.mjs --run 3`

Platform tested on: <!-- Windows / macOS (never compiled — say so) -->

## Checklist

- [ ] I have signed the [CLA](../blob/main/CLA.md).
- [ ] New first-party files carry `// SPDX-License-Identifier: AGPL-3.0-or-later`.
- [ ] No new dependency, or the dependency was discussed in an issue first and is recorded in
      [`THIRD-PARTY-NOTICES.md`](../blob/main/THIRD-PARTY-NOTICES.md) with its licence and source.
- [ ] No hardcoded colours, spacing, radii or type — everything through the token layer.
- [ ] No secret can reach a log, a panic message, a `Debug` impl or an error string.
- [ ] No new outbound network request.
- [ ] I did not edit `tests/acceptance/` or `scripts/verify-v1.mjs`.
- [ ] Comments I added are true, including any that describe a security property.
- [ ] If I touched macOS code, `MACOS-UNVERIFIED.md` gained a row in this same commit.
