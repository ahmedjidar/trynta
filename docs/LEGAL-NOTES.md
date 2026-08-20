# Legal notes

<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

Four things that a cryptographic product published as open source has to have an answer to, and
that are easier to decide before publication than after.

**None of this is legal advice.** It is a description of the position the repository currently
takes, written by someone unqualified to tell you whether that position is correct where you live.
Each section says plainly what has and has not been checked.

---

## 1. Export control

Trynta implements strong cryptography — Argon2id, XChaCha20-Poly1305, Ed25519, X25519 — and
publishing it makes it available for download worldwide.

**What is generally true of open-source cryptography:**

- Under the United States Export Administration Regulations, publicly available encryption _source
  code_ is treated very differently from a compiled product. §734.7 removes published information
  from the scope of the EAR, and §742.15(b) sets out a notification procedure for encryption source
  code made publicly available — historically an email to BIS and the ENC Encryption Request
  Coordinator at the time of first publication. Object code compiled from published source is
  generally treated the same way.
- The Wassenaar Arrangement has a "public domain"/publicly-available carve-out that most signatory
  implementations mirror, which is why the great majority of open-source cryptography ships without
  a licence.

**What has not been determined:** which jurisdiction's rules actually apply here. That depends on
where the copyright holder is resident and where the artefacts are hosted, and this repository
does not record either. The EU dual-use regulation, the UK's implementation and others all have
their own publicly-available exemptions with their own wording.

**Recommended before the repository is made public:** confirm the notification requirement, if any,
for your jurisdiction. For a US-based publisher this has historically been one email; for others
the exemption is usually automatic. It is a cheap thing to check and an expensive thing to
discover you needed.

**Note also:** publishing binaries is a different act from publishing source. If GitHub Releases
carries installers, that is distribution of compiled encryption software and is worth confirming
separately.

## 2. Trademarks

### Other people's

Trynta bundles 3,952 brand marks for 3,778 services. These are trademarks of their respective
owners, used **nominatively** — to identify a service the user already holds an account with. That
is the same use a browser bookmark bar makes of a favicon.

The position, stated in [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md) and reproduced in the
README and the application itself:

> Service logos are the trademarks of their respective owners and are used solely to identify those
> services within the interface. Their presence implies no affiliation with or endorsement by those
> companies.

Two things make this stronger than a disclaimer alone. Marks are never recoloured, redrawn or
merged — the build strips metadata and normalises the viewBox and does nothing else. And every mark
traces to a row in `src-tauri/assets/icon-map.tsv` carrying its source and licence, so a takedown
request can be answered precisely rather than by removing a collection.

**A file licence is not a trademark licence.** A CC0 SVG of a company's logo means the _drawing_ is
freely licensed; it says nothing about the _mark_. Nominative use is the basis here, not the file
licence, and that is true of every one of the 3,778.

**If a rights holder objects,** the practical answer is to remove that key from the map and rebuild
— the item falls back to a generated mark and nothing else changes. Worth knowing before the first
email arrives.

### Yours

**AGPL-3.0 grants no trademark rights.** §7(e) explicitly allows a licence to decline to grant
rights in trade names, trademarks or service marks, and the GPL family has never granted them.
Anyone may fork Trynta and must be able to distribute the fork — but the name "Trynta" and the mark
are not covered by the copyright licence.

**Not determined:** whether "Trynta" is registered as a trademark in any jurisdiction, or whether
it conflicts with an existing mark. That search has not been done. It is worth doing before the
name is attached to anything commercial, and it is much cheaper to find a conflict now than after a
release.

**Consider adding a `TRADEMARK.md`** if forks become likely, stating what a fork may and may not
call itself. Mozilla's and Rust's are the usual models. Not added here, because a policy for a
situation that has not arisen is guesswork.

## 3. Patents

Trynta implements published, standard cryptographic primitives, all of which are decades old or
were published royalty-free. No novel construction is used anywhere — that is an explicit rule in
[`CLAUDE.md`](../CLAUDE.md) §4.1, not an accident.

**AGPL-3.0 carries a patent grant.** §11 grants a non-exclusive, worldwide, royalty-free patent
licence from each contributor covering their contributions, and includes protection against
patent-based termination. That is one of the reasons GPLv3-family licences are preferred over
GPLv2 for anything cryptographic.

**The CLA adds a second, matching grant** — [`CLA.md`](../CLA.md) clause 3, with defensive
termination — so contributions carry a patent licence to the Maintainer and to downstream
recipients, and a contributor who later sues over their own contribution loses their grant.

**No freedom-to-operate search has been done.** That is normal for a project of this size and worth
stating rather than implying otherwise.

## 4. The AGPL network clause, and what it means for the sync relay

This is the one to decide now rather than later, because it constrains a design that has not been
built yet.

**What §13 says.** If you modify the Program and let users interact with it _remotely through a
computer network_, you must offer those users the Corresponding Source of your modified version.
This is the clause that distinguishes the AGPL from the GPL, and the reason the AGPL was chosen.

**Today it binds nobody.** Trynta is a desktop application. It talks to two hosts — the HIBP range
API and the update manifest — and neither is ours. No one interacts with Trynta over a network, so
§13 is dormant.

**What changes with V3.** A sync relay is server software. Three cases, and they are genuinely
different:

| Scenario                                                                                                                                     | Consequence                                                                                                                     |
| -------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| The relay is a **separate program** that stores opaque ciphertext blobs and shares no code with the client                                   | §13 does not reach it. It is a different work, and you may licence it however you like — including keeping it closed.           |
| The relay **reuses code from this repository** — the crypto crate, the store crate, shared types — and users interact with it over a network | §13 applies. Running it publicly obliges offering its complete corresponding source to its users, including your modifications. |
| Someone **else** takes this repository, builds a hosted service from it, and offers it to their users                                        | §13 applies to them. This is exactly what the AGPL is for and the reason it was chosen over the GPL.                            |

**The practical consequence, stated so it is not discovered late:** if the intention is to run a
hosted relay commercially without publishing its source, the relay must not share code with the
AGPL client — or it must be covered by the relicensing right the [CLA](../CLA.md) preserves. Those
are the only two routes, and the CLA is the one that keeps both open.

Note that the client being AGPL does not force the relay to be AGPL merely because they talk to
each other. Communication across a documented protocol is not, on its own, a derivative work.
Sharing crate code is.

**Deciding now costs nothing. Deciding after the relay exists may cost a rewrite.**

---

## Summary of what is unresolved

| Item                                                         | State                                                                |
| ------------------------------------------------------------ | -------------------------------------------------------------------- |
| Export-control notification for the publisher's jurisdiction | **Not determined.** Jurisdiction not recorded in this repository.    |
| Trademark search for the name "Trynta"                       | **Not done.**                                                        |
| Freedom-to-operate patent search                             | **Not done.** Standard for a project this size.                      |
| Six CC-BY-SA 2.5/3.0 bundled icons                           | **Open.** See [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md). |
| Installers ship no licence or attribution text               | **Open.** See [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md). |
| Relay licensing strategy                                     | **Decide before V3 design work begins.**                             |
