; Installer der nativen Wavely-Testversion (ohne WebView2).
; Installiert nur für den angemeldeten Nutzer (keine Admin-Rechte), neben der normalen Wavely.
; Aufruf: makensis /DVERSION=0.17.0-native.1 /DSOURCE=<Ordner mit wavely-native.exe und yt-dlp.exe> /DOUTFILE=<Ziel.exe>

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

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
!define OLD_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Wavely"

; Aufruf aus der alten Wavely als Update (der Tauri-Updater übergibt /UPDATE): still installieren
; und die alte Wavely danach ersetzen. Per Doppelklick bleibt die alte App daneben bestehen.
Var FromOldWavely

Function .onInit
  StrCpy $FromOldWavely 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/UPDATE" $R1
  ${IfNot} ${Errors}
    StrCpy $FromOldWavely 1
    SetSilent silent
  ${EndIf}
FunctionEnd

Section "Wavely Native"
  ; Bei Updates beendet sich Wavely Native selbst, bevor der Installer startet
  SetOutPath "$INSTDIR"
  File "${SOURCE}\wavely-native.exe"
  File "${SOURCE}\yt-dlp.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\Wavely Native.lnk" "$INSTDIR\wavely-native.exe"
  ; Symbol-Zwischenspeicher von Windows auffrischen, damit Taskleiste und Startmenü das neue Logo zeigen
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  nsExec::Exec '"$SYSDIR\ie4uinit.exe" -show'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Wavely Native"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "ZyroMysty"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\wavely-native.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ; Alte Wavely ersetzen: beenden und still deinstallieren. Einstellungen und Daten bleiben im
  ; Ordner com.wavely.app, Wavely Native übernimmt sie beim ersten Start.
  ${If} $FromOldWavely == 1
    ReadRegStr $R2 HKCU "${OLD_KEY}" "InstallLocation"
    ; Anführungszeichen entfernen
    StrCpy $R3 $R2 1
    ${If} $R3 == '"'
      StrCpy $R2 $R2 "" 1
      StrCpy $R2 $R2 -1
    ${EndIf}
    ${If} $R2 != ""
    ${AndIf} ${FileExists} "$R2\uninstall.exe"
      Sleep 1500
      nsExec::Exec 'taskkill /F /IM wavely.exe'
      Sleep 500
      ExecWait '"$R2\uninstall.exe" /S _?=$R2'
      Delete "$R2\uninstall.exe"
      RMDir "$R2"
    ${EndIf}
  ${EndIf}
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
