; installer.nsi - HFPredict-<version>-windows-<arch>-setup.exe, built by
; packaging/windows/build.sh with makensis from the folder it zips:
;   makensis -DVERSION=0.1.0 -DARCH=x86_64 -DSRC=<HFPredict folder>
;            -DICON=<hfpredict.ico> -DOUTFILE=<setup.exe> installer.nsi
;
; Installs for the current user only (no administrator rights) into
; %LOCALAPPDATA%\Programs\HFPredict, with a Start menu shortcut and an entry
; in Settings -> Apps to uninstall it. The installer's own texts are NSIS's
; translations, picked from the Windows display language.
;
; The app updates itself by running this installer as
;   setup.exe /S /RELAUNCH
; silently over the installed copy: it waits for the app to quit, installs,
; then opens it again (desktop/src/update.rs).

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinVer.nsh"
!include "x64.nsh"
!include "FileFunc.nsh"

!define APP_ID "io.alvaroedo.HFPredict"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "HFPredict"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\HFPredict"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "HFPredict ${VERSION}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "HFPredict"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "HFPredict ${VERSION} (${ARCH}) setup"
VIAddVersionKey "LegalCopyright" "Álvaro Edo Martínez, GPL-3.0-or-later"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\bin\hfpredict.exe"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; The app's languages; English first, the fallback.
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Spanish"
!insertmacro MUI_LANGUAGE "Catalan"
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "Japanese"
!insertmacro MUI_LANGUAGE "Arabic"

Function .onInit
    ; GTK 4 needs Windows 10 or newer. The arm64 build runs only on ARM64;
    ; the x86_64 one also runs on ARM64 under Windows' own emulation.
    ${IfNot} ${AtLeastWin10}
        MessageBox MB_OK|MB_ICONSTOP "HFPredict needs Windows 10 or newer."
        Abort
    ${EndIf}
    !if "${ARCH}" == "arm64"
        ${IfNot} ${IsNativeARM64}
            MessageBox MB_OK|MB_ICONSTOP "This is HFPredict for ARM64 Windows. Install the x86_64 one on this computer."
            Abort
        ${EndIf}
    !else
        ${IfNot} ${IsNativeAMD64}
        ${AndIfNot} ${IsNativeARM64}
            MessageBox MB_OK|MB_ICONSTOP "HFPredict needs 64-bit Windows."
            Abort
        ${EndIf}
    !endif
FunctionEnd

Section "HFPredict"
    ; An update replaces the previous files, so no stale library stays
    ; behind; only where HFPredict is installed already.
    ${If} ${FileExists} "$INSTDIR\bin\hfpredict.exe"
        ; The app that started this update is still quitting: wait (up to
        ; 30 s) until its program can be removed.
        StrCpy $0 0
        ${Do}
            ClearErrors
            Delete "$INSTDIR\bin\hfpredict.exe"
            ${IfNot} ${Errors}
                ${Break}
            ${EndIf}
            IntOp $0 $0 + 1
            ${If} $0 >= 60
                ${Break}
            ${EndIf}
            Sleep 500
        ${Loop}
        RMDir /r "$INSTDIR\bin"
        RMDir /r "$INSTDIR\lib"
        RMDir /r "$INSTDIR\share"
    ${EndIf}
    SetOutPath "$INSTDIR"
    File /r "${SRC}\*.*"
    WriteUninstaller "$INSTDIR\uninstall.exe"

    CreateShortcut "$SMPROGRAMS\HFPredict.lnk" "$INSTDIR\bin\hfpredict.exe" "" "$INSTDIR\bin\hfpredict.exe" 0

    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "HFPredict"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Álvaro Edo Martínez"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://code.alvaroedo.io/alvaroemtnez/HFPredict"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\bin\hfpredict.exe"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1

    ; An update from the app itself: open it again.
    ${GetParameters} $1
    ClearErrors
    ${GetOptions} $1 "/RELAUNCH" $2
    ${IfNot} ${Errors}
        Exec '"$INSTDIR\bin\hfpredict.exe"'
    ${EndIf}
SectionEnd

Section "Uninstall"
    Delete "$SMPROGRAMS\HFPredict.lnk"
    ; Only what the installer put there; the folder goes if nothing else is
    ; left in it. Settings and the map cache, in the user's profile, stay.
    RMDir /r "$INSTDIR\bin"
    RMDir /r "$INSTDIR\lib"
    RMDir /r "$INSTDIR\share"
    Delete "$INSTDIR\LICENSE"
    Delete "$INSTDIR\NOTICE"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
