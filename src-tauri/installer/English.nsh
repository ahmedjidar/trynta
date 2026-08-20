; SPDX-License-Identifier: AGPL-3.0-or-later
;
; Installer and uninstaller text, replacing Tauri's default English strings.
;
; A complete replacement rather than a patch — `nsis.customLanguageFiles` substitutes
; the whole file, so every string the template references has to be here or the build
; fails on an undefined `LangString`.
;
; Two classes of change, and the reasons are different:
;
; 1. **"Unable to uninstall!" told the user nothing.** It is raised by
;    `installer.nsi` when the old uninstaller either returned non-zero or left
;    `Trynta.exe` on disk, and those have entirely different causes — a declined
;    "close the app" prompt, or a file lock from the running app, an antivirus scan or
;    an Explorer preview. The installer cannot tell which, so the message now names
;    both and says what to do about each. A dead end becomes a next step.
;
; 2. **"Delete the application data" did not say what it deletes.** For a password
;    manager the application data *is* the user's vault, and deleting it is
;    unrecoverable without a backup. The checkbox label now says so in the words that
;    matter, and the uninstall hook shows the full consequence before acting.
;
; Every other string is Tauri's own, reproduced unchanged.

LangString addOrReinstall ${LANG_ENGLISH} "Add/Reinstall components"
LangString alreadyInstalled ${LANG_ENGLISH} "Already Installed"
LangString alreadyInstalledLong ${LANG_ENGLISH} "${PRODUCTNAME} ${VERSION} is already installed. Select the operation you want to perform and click Next to continue.$\n$\nNote: 'Uninstall before installing' runs the uninstaller, which will ask whether to keep or delete your vault. Your vault is NOT removed unless you choose to remove it."

; Raised when the app is found running and the installer is about to give up.
LangString appRunning ${LANG_ENGLISH} "{{product_name}} is still running, so its files cannot be replaced.$\n$\nClose {{product_name}} — check the system tray and the taskbar — then run this installer again."

; Raised before killing the process. Says what closing it costs, because an unsaved
; edit in the app is a real thing to lose.
LangString appRunningOkKill ${LANG_ENGLISH} "{{product_name}} is running and has to be closed before it can be replaced.$\n$\nClick OK to close it now. Your vault is not affected — it is saved as you go — but anything you are part-way through typing will be lost.$\n$\nClick Cancel to stop and close it yourself."

LangString chooseMaintenanceOption ${LANG_ENGLISH} "Choose the maintenance option to perform."
LangString choowHowToInstall ${LANG_ENGLISH} "Choose how you want to install ${PRODUCTNAME}."
LangString createDesktop ${LANG_ENGLISH} "Create desktop shortcut"
LangString dontUninstall ${LANG_ENGLISH} "Do not uninstall"
LangString dontUninstallDowngrade ${LANG_ENGLISH} "Do not uninstall (Downgrading without uninstall is disabled for this installer)"

LangString failedToKillApp ${LANG_ENGLISH} "{{product_name}} could not be closed.$\n$\nClose it yourself — check the system tray and the taskbar — then run this installer again. If it is not visibly running, end the '{{product_name}}' process in Task Manager, or restart Windows."

LangString installingWebview2 ${LANG_ENGLISH} "Installing WebView2..."
LangString newerVersionInstalled ${LANG_ENGLISH} "A newer version of ${PRODUCTNAME} is already installed. Installing an older version is not recommended.$\n$\nIf you want to go back, uninstall the current version first — the uninstaller will ask whether to keep or delete your vault. Select the operation you want to perform and click Next to continue."
LangString older ${LANG_ENGLISH} "older"
LangString olderOrUnknownVersionInstalled ${LANG_ENGLISH} "An $R4 version of ${PRODUCTNAME} is installed. Uninstalling it first is recommended.$\n$\n'Uninstall before installing' runs the uninstaller, which asks whether to keep or delete your vault. Your vault is NOT removed unless you choose to remove it. Select the operation you want to perform and click Next to continue."
LangString silentDowngrades ${LANG_ENGLISH} "Downgrades are disabled for this installer, can't proceed with the silent installer, please use the graphical interface installer instead.$\n"

; The message this whole file exists for. Both causes, and what to do about each.
LangString unableToUninstall ${LANG_ENGLISH} "The previous version of ${PRODUCTNAME} could not be removed, so this installer has stopped. Your vault has not been touched.$\n$\nThe usual cause is that a file was still in use. Try, in order:$\n$\n  1.  Close ${PRODUCTNAME} if it is open — check the system tray and the taskbar — and run this installer again.$\n  2.  If it is not running, something else is holding its files: an antivirus scan, a backup tool, or an open Explorer preview of the install folder. Close that folder and try again.$\n  3.  Restart Windows and run the installer again. This clears any remaining lock.$\n  4.  If it still fails, uninstall from Settings > Apps > Installed apps, then run this installer.$\n$\nYou can also choose 'Add/Reinstall components' on the previous screen, which installs over the existing version without uninstalling it."

LangString uninstallApp ${LANG_ENGLISH} "Uninstall ${PRODUCTNAME}"
LangString uninstallBeforeInstalling ${LANG_ENGLISH} "Uninstall before installing (your vault is kept unless you say otherwise)"
LangString unknown ${LANG_ENGLISH} "unknown"

LangString webview2AbortError ${LANG_ENGLISH} "WebView2 could not be installed, and ${PRODUCTNAME} cannot run without it.$\n$\nWebView2 is Microsoft's web runtime and ships with Windows 11. Check your internet connection and run this installer again, or install 'Microsoft Edge WebView2 Runtime' from Microsoft's site first."
LangString webview2DownloadError ${LANG_ENGLISH} "WebView2 could not be downloaded - $0$\n$\nCheck your internet connection, or any proxy or firewall that might be blocking it, and try again."
LangString webview2DownloadSuccess ${LANG_ENGLISH} "WebView2 bootstrapper downloaded successfully"
LangString webview2Downloading ${LANG_ENGLISH} "Downloading WebView2 bootstrapper..."
LangString webview2InstallError ${LANG_ENGLISH} "WebView2 installation failed with exit code $1$\n$\nTry installing 'Microsoft Edge WebView2 Runtime' from Microsoft's site, then run this installer again."
LangString webview2InstallSuccess ${LANG_ENGLISH} "WebView2 installed successfully"

; The checkbox on the uninstall confirmation page. Names the thing, not the category.
LangString deleteAppData ${LANG_ENGLISH} "Also delete my vault and all my saved passwords"
