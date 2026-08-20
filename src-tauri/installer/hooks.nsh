; SPDX-License-Identifier: AGPL-3.0-or-later
;
; Installer and uninstaller hooks (`nsis.installerHooks`).
;
; Tauri's NSIS template exposes four macros — PREINSTALL, POSTINSTALL, PREUNINSTALL,
; POSTUNINSTALL — and everything here hangs off two of them. The template itself is
; untouched: forking it would mean re-merging on every Tauri upgrade, and the two things
; that needed fixing are both reachable from a hook.
;
; ## Why the uninstaller had to be changed at all
;
; The template already draws a "Delete the application data" checkbox and, if it is
; ticked, removes `$APPDATA\${BUNDLEID}` and `$LOCALAPPDATA\${BUNDLEID}`.
;
; **Trynta does not store anything at either path.** `platform::paths` builds its data
; directory from the *product name* — `%APPDATA%\Trynta` — because SPEC-V1 §8 specifies
; a human-readable directory, not a reverse-DNS one. So the template's checkbox pointed
; at `%APPDATA%\dev.trynta.desktop`, which has never existed.
;
; That failed in the worse of the two possible directions. Nothing was ever destroyed —
; but a user who ticked "delete the application data" to clean the machine was left with
; their entire encrypted vault still on disk, having been told it was gone. For a
; password manager that is the wrong side to be wrong on.
;
; ## What replaces it
;
; PREUNINSTALL asks, in one dialog, with the consequence spelled out and **keep as the
; default**. Not the template's checkbox, because a checkbox on a confirmation page is
; easy to leave in whatever state it was in; and not silence, because both silent
; answers are bad. Deleting a vault silently destroys data. Keeping one silently leaves
; an encrypted copy of every password on a machine the user believes is clean.
;
; The delete path also asks a second time. A vault cannot be recovered without a
; backup, and no amount of message text makes a single click reversible.

!macro NSIS_HOOK_PREINSTALL
  ; Nothing. Kept declared so the template's !ifmacrodef finds a matching pair and a
  ; future addition has an obvious home.
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

; ── Uninstall ───────────────────────────────────────────────────────────────

Var TryntaVaultDir
Var TryntaDeleteVault

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $TryntaDeleteVault "0"

  ; The directory the application actually writes to. `SetShellVarContext current`
  ; because the install is per-user and $APPDATA must resolve to the invoking user's
  ; roaming profile, not the elevated account's if this ever runs elevated.
  SetShellVarContext current
  StrCpy $TryntaVaultDir "$APPDATA\${PRODUCTNAME}"

  ; An update replaces files and keeps data. Asking about the vault during an update
  ; would be asking the wrong question: the user chose "install", not "remove".
  ${If} $UpdateMode = 1
    Goto trynta_vault_done
  ${EndIf}

  ; Nothing to ask about if there is no vault.
  ${IfNot} ${FileExists} "$TryntaVaultDir\vault.db"
    Goto trynta_vault_done
  ${EndIf}

  ; A silent or passive uninstall keeps the vault. The safe answer is the one that
  ; loses nothing, and an unattended run has nobody to ask.
  ${If} ${Silent}
  ${OrIf} $PassiveMode = 1
    Goto trynta_vault_done
  ${EndIf}

  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 \
    "Keep your vault?$\n$\n\
    Your passwords are stored at:$\n\
    $TryntaVaultDir$\n$\n\
    Uninstalling ${PRODUCTNAME} does not need to remove them.$\n$\n\
    NO  —  Keep my vault (recommended). The folder above is left alone. Reinstall \
${PRODUCTNAME} later and everything is exactly as you left it. Nothing can read it \
without your master password.$\n$\n\
    YES  —  Delete my vault. Every password, note, card and one-time code is erased \
from this computer. This CANNOT be undone. Unless you have exported a backup, they \
are gone permanently.$\n$\n\
    Delete the vault?" \
    /SD IDNO IDYES trynta_vault_confirm IDNO trynta_vault_done

  trynta_vault_confirm:
    ; Asked twice on purpose. The first dialog explains; this one is the point of no
    ; return, so it states only the irreversible part and defaults to cancelling.
    MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION|MB_DEFBUTTON2 \
      "Delete every password stored in ${PRODUCTNAME}?$\n$\n\
      $TryntaVaultDir$\n$\n\
      There is no undo and no recycle bin for this. Without a backup file, the \
contents cannot be recovered by anyone, including us — the vault is encrypted with a \
key derived from your master password and we hold no copy.$\n$\n\
      Click Cancel to keep it." \
      /SD IDCANCEL IDOK trynta_vault_delete
    Goto trynta_vault_done

  trynta_vault_delete:
    StrCpy $TryntaDeleteVault "1"

  trynta_vault_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Runs after the template's own cleanup. The template's checkbox targets
  ; `$APPDATA\${BUNDLEID}`, which Trynta never writes to, so it removes nothing and
  ; the decision above is the only one that has any effect.
  ${If} $TryntaDeleteVault == "1"
    SetShellVarContext current
    RMDir /r "$APPDATA\${PRODUCTNAME}"
    ${If} ${FileExists} "$APPDATA\${PRODUCTNAME}\vault.db"
      ; Say so rather than reporting a clean uninstall over a vault that is still
      ; there. A user who asked for deletion and did not get it needs to know.
      MessageBox MB_ICONEXCLAMATION \
        "Your vault could not be deleted and is still on this computer:$\n$\n\
        $APPDATA\${PRODUCTNAME}$\n$\n\
        Something is holding the files open — most likely ${PRODUCTNAME} is still \
running, or a backup or antivirus tool is reading them. Close it, then delete that \
folder yourself. Everything else has been removed."
    ${EndIf}
  ${EndIf}
!macroend
