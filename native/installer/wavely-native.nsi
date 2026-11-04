; Installer der nativen Wavely-Testversion (ohne WebView2).
; Installiert nur für den angemeldeten Nutzer (keine Admin-Rechte), neben der normalen Wavely.
; Aufruf: makensis /DVERSION=0.17.0-native.1 /DSOURCE=<Ordner mit wavely-native.exe und yt-dlp.exe> /DOUTFILE=<Ziel.exe>

Unicode true
!include "MUI2.nsh"

Name "Wavely Native"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Wavely Native"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON "..\..\src-tauri\icons\icon.ico"
!define MUI_UNICON "..\..\src-tauri\icons\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\wavely-native.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Wavely Native starten"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "German"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\WavelyNative"

Section "Wavely Native"
  ; Bei Updates beendet sich Wavely Native selbst, bevor der Installer startet
  SetOutPath "$INSTDIR"
  File "${SOURCE}\wavely-native.exe"
  File "${SOURCE}\yt-dlp.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\Wavely Native.lnk" "$INSTDIR\wavely-native.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Wavely Native (Testversion)"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "ZyroMysty"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\wavely-native.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ; Stilles Update (/S): danach gleich wieder starten
  IfSilent 0 +2
    Exec '"$INSTDIR\wavely-native.exe"'
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\wavely-native.exe"
  Delete "$INSTDIR\yt-dlp.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Wavely Native.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Wavely Native"
SectionEnd
