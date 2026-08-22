# Trynta Individual Contributor License Agreement

**Version 1.0**

Thank you for your interest in contributing to Trynta ("the Project"), maintained by Ahmed Amin ("the
Maintainer").

This agreement clarifies the intellectual-property licence granted with contributions from any
person or entity. You keep the copyright in your contribution — this is a **licence, not an
assignment**, and nothing here takes your work away from you or stops you using it however you
like elsewhere.

Please read this carefully. You accept and agree to the following terms for your present and
future contributions to the Project. Except for the licence granted here, you reserve all right,
title and interest in and to your contributions.

---

## Why this exists, and what it lets the Maintainer do

Trynta is released under the [GNU Affero General Public License v3.0 or later](LICENSE). The
Maintainer may in future also offer Trynta, or parts of it, under a different licence — including
a commercial one — to people who cannot or will not accept the AGPL's terms.

That is only possible if the Maintainer holds a licence broad enough to sublicense every line in
the tree. Contributions accepted without such a grant cannot later be relicensed without tracking
down every contributor and asking. **That door closes the first time a contribution lands without
one**, which is why this agreement exists now rather than later.

Concretely, clause 2 below lets the Maintainer:

- keep distributing the Project under AGPL-3.0-or-later, and
- additionally distribute your contribution under other terms, including proprietary or
  commercial terms, without asking you again.

It does **not** let the Maintainer stop you using your own contribution, and it does not remove
the AGPL grant already made to everyone who has received the Project.

If that is not acceptable to you, please say so before opening a pull request. It is a reasonable
position and it is better raised early than after you have written the code.

---

## Terms

### 1. Definitions

**"You"** (or **"Your"**) means the copyright owner, or the legal entity authorised by the
copyright owner, that is entering into this Agreement with the Maintainer.

**"Contribution"** means any original work of authorship, including any modifications or additions
to an existing work, that is intentionally submitted by You to the Maintainer for inclusion in, or
documentation of, the Project. "Submitted" means any form of electronic, verbal or written
communication sent to the Maintainer or its representatives, including but not limited to
communication on electronic mailing lists, source-code control systems and issue-tracking systems
that are managed by, or on behalf of, the Maintainer for the purpose of discussing and improving
the Project — but excluding communication that is conspicuously marked, or otherwise designated in
writing by You, as **"Not a Contribution."**

### 2. Grant of Copyright Licence

Subject to the terms and conditions of this Agreement, You hereby grant to the Maintainer and to
recipients of software distributed by the Maintainer a perpetual, worldwide, non-exclusive,
no-charge, royalty-free, irrevocable copyright licence to reproduce, prepare derivative works of,
publicly display, publicly perform, **sublicense**, and distribute Your Contributions and such
derivative works.

For the avoidance of doubt, the sublicensing right granted above expressly includes the right to
distribute Your Contribution under licence terms other than AGPL-3.0-or-later, including
proprietary and commercial terms, at the Maintainer's sole discretion.

You retain all right, title and interest in and to Your Contributions, and this Agreement does not
transfer ownership of them.

### 3. Grant of Patent Licence

Subject to the terms and conditions of this Agreement, You hereby grant to the Maintainer and to
recipients of software distributed by the Maintainer a perpetual, worldwide, non-exclusive,
no-charge, royalty-free, irrevocable (except as stated in this section) patent licence to make,
have made, use, offer to sell, sell, import and otherwise transfer the Work.

This licence applies only to those patent claims licensable by You that are necessarily infringed
by Your Contribution alone, or by combination of Your Contribution with the Project to which it
was submitted.

If any entity institutes patent litigation against You or any other entity — including a
cross-claim or counterclaim in a lawsuit — alleging that Your Contribution, or the Project to
which You contributed, constitutes direct or contributory patent infringement, then any patent
licences granted to that entity under this Agreement for that Contribution or Project terminate as
of the date such litigation is filed.

### 4. You are entitled to grant this

You represent that You are legally entitled to grant the above licences.

If Your employer has rights to intellectual property that You create, You represent that You have
received permission to make Contributions on behalf of that employer, that Your employer has
waived such rights for Your Contributions to the Project, or that Your employer has executed a
separate corporate agreement with the Maintainer.

### 5. The work is Yours, and third-party material is disclosed

You represent that each of Your Contributions is Your original creation.

You represent that Your Contribution submissions include complete details of any third-party
licence or other restriction — including but not limited to related patents and trademarks — of
which You are personally aware and which are associated with any part of Your Contributions.

This matters more here than in most projects. Trynta bundles a large number of third-party assets
and every one of them is recorded in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) with its
licence and source. A contribution that adds an asset, a dependency, or a block of code from
elsewhere must say so, and must say under what terms.

### 6. No support, no warranty

You are not expected to provide support for Your Contributions, except to the extent You wish to.
You may provide support for free, for a fee, or not at all.

Unless required by applicable law or agreed to in writing, You provide Your Contributions on an
**"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND**, either express or implied,
including without limitation any warranties or conditions of TITLE, NON-INFRINGEMENT,
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.

### 7. Third-party submissions

Should You wish to submit work that is not Your original creation, You may submit it to the
Maintainer separately from any Contribution, identifying the complete details of its source and of
any licence or other restriction (including but not limited to related patents, trademarks and
licence agreements) of which You are personally aware, and conspicuously marking the work as
**"Submitted on behalf of a third-party: [named here]"**.

### 8. Tell the Maintainer if this stops being true

You agree to notify the Maintainer of any facts or circumstances of which You become aware that
would make these representations inaccurate in any respect.

---

## How to sign

Add a line to the end of this file in your first pull request, in this form:

```
Signed: Your Name <your@email.example> (github: @yourhandle) — YYYY-MM-DD
```

Committing that line to the repository, in a pull request opened from your own account, is your
signature. One signature covers all of your future contributions; you do not sign again per pull
request.

---

## Provenance and a caveat worth stating plainly

This agreement is **adapted from the Apache Software Foundation's Individual Contributor License
Agreement (ICLA) v2.0**, which is the most widely used and most widely reviewed template of its
kind. Its clause structure — definitions, copyright grant, patent grant with defensive
termination, entitlement representation, originality and third-party disclosure, warranty
disclaimer, third-party submissions, notification — is followed directly, and clauses 2, 3, 6 and
7 keep the ASF's wording almost verbatim.

Two things are changed from the ASF original, both deliberate:

1. **The counterparty is an individual, not a foundation.** The grant runs to the Maintainer rather
   than to the ASF.
2. **The relicensing intent is made explicit** in clause 2. The ASF text already grants
   `sublicense`, which is what makes relicensing legally possible, but it leaves the consequence
   for the reader to infer. Spelling it out is the honest thing to do when the whole point of
   asking you to sign is to keep a commercial option open.

**The Linux kernel's Developer Certificate of Origin (DCO) was considered and rejected**, for one
reason: the DCO is a _certificate of provenance_, not a licence grant. It asserts that you have
the right to submit the code under the project's existing licence. It grants no sublicensing
right, so it would leave the Project permanently unable to offer anything other than AGPL. A DCO
is the better choice for a project that will never relicense — and if that were the plan here,
this file would not exist.

> **This is not legal advice, and it has not been reviewed by a lawyer.** It is an adaptation of a
> well-established template by someone who is not qualified to give an opinion on its
> enforceability in any particular jurisdiction. If the commercial option genuinely matters to
> you, have a solicitor read it before you rely on it.

---

## Signatures

<!-- Add your line below, newest last. -->
