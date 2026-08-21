Unicode True
RequestExecutionLevel user

!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !define VERSION "1.0.1"
!endif
!ifndef SOURCE_EXE
  !error "SOURCE_EXE must point to cursor-zh-launcher.exe"
!endif
!ifndef PROJECT_ROOT
  !define PROJECT_ROOT ".."
!endif
!ifndef OUTPUT_FILE
  !define OUTPUT_FILE "CursorZhLauncher-${VERSION}-Setup.exe"
!endif

!define PRODUCT_NAME "Cursor 中文启动器"
!define PRODUCT_ID "CursorZhLauncher"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_ID}"
!define CURSOR_UNINSTALL_KEY \
  "Software\Microsoft\Windows\CurrentVersion\Uninstall\{DADADADA-ADAD-ADAD-ADAD-ADADADADADAD}}_is1"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\CursorZhLauncher"
InstallDirRegKey HKCU "Software\CursorZhLauncher" "InstallDir"
BrandingText "非官方 Cursor 社区中文化工具"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\cursor-zh-launcher.exe"
!define MUI_FINISHPAGE_RUN_TEXT "启动 Cursor 中文启动器"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PROJECT_ROOT}\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"

Var CursorIconPath

Function ResolveCursorIcon
  StrCpy $CursorIconPath "$INSTDIR\cursor-zh-launcher.exe"

  SetRegView 64
  ReadRegStr $0 HKCU "${CURSOR_UNINSTALL_KEY}" "InstallLocation"
  StrCmp $0 "" cursor_icon_registry_hklm_64
  StrCpy $1 "$0\Cursor.exe"
  IfFileExists "$1" cursor_icon_found

cursor_icon_registry_hklm_64:
  ReadRegStr $0 HKLM "${CURSOR_UNINSTALL_KEY}" "InstallLocation"
  StrCmp $0 "" cursor_icon_registry_32
  StrCpy $1 "$0\Cursor.exe"
  IfFileExists "$1" cursor_icon_found

cursor_icon_registry_32:
  SetRegView 32
  ReadRegStr $0 HKCU "${CURSOR_UNINSTALL_KEY}" "InstallLocation"
  StrCmp $0 "" cursor_icon_registry_hklm_32
  StrCpy $1 "$0\Cursor.exe"
  IfFileExists "$1" cursor_icon_found

cursor_icon_registry_hklm_32:
  ReadRegStr $0 HKLM "${CURSOR_UNINSTALL_KEY}" "InstallLocation"
  StrCmp $0 "" cursor_icon_user_install
  StrCpy $1 "$0\Cursor.exe"
  IfFileExists "$1" cursor_icon_found

cursor_icon_user_install:
  StrCpy $1 "$LOCALAPPDATA\Programs\cursor\Cursor.exe"
  IfFileExists "$1" cursor_icon_found
  StrCpy $1 "$LOCALAPPDATA\Programs\Cursor\Cursor.exe"
  IfFileExists "$1" cursor_icon_found
  StrCpy $1 "$PROGRAMFILES64\Cursor\Cursor.exe"
  IfFileExists "$1" cursor_icon_found
  StrCpy $1 "$PROGRAMFILES32\Cursor\Cursor.exe"
  IfFileExists "$1" cursor_icon_found cursor_icon_done

cursor_icon_found:
  StrCpy $CursorIconPath "$1"

cursor_icon_done:
  SetRegView 32
FunctionEnd

Section "Cursor 中文启动器（必需）" SecMain
  SectionIn RO
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /oname=cursor-zh-launcher.exe "${SOURCE_EXE}"
  File /oname=LICENSE.txt "${PROJECT_ROOT}\LICENSE"
  File /oname=README.md "${PROJECT_ROOT}\README.md"
  Call ResolveCursorIcon

  CreateDirectory "$SMPROGRAMS\Cursor 中文启动器"
  CreateShortcut "$SMPROGRAMS\Cursor 中文启动器\Cursor 中文启动器.lnk" \
    "$INSTDIR\cursor-zh-launcher.exe" "" "$CursorIconPath" 0
  CreateShortcut "$DESKTOP\Cursor 中文启动器.lnk" \
    "$INSTDIR\cursor-zh-launcher.exe" "" "$CursorIconPath" 0

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\CursorZhLauncher" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Cursor 中文启动器 contributors"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section /o "资源管理器右键入口" SecContextMenu
  WriteRegStr HKCU "Software\Classes\*\shell\CursorZhLauncher" "" \
    "使用 Cursor 中文启动器打开"
  WriteRegStr HKCU "Software\Classes\*\shell\CursorZhLauncher" "Icon" \
    "$CursorIconPath"
  WriteRegStr HKCU "Software\Classes\*\shell\CursorZhLauncher\command" "" \
    '"$INSTDIR\cursor-zh-launcher.exe" "%1"'

  WriteRegStr HKCU "Software\Classes\Directory\shell\CursorZhLauncher" "" \
    "使用 Cursor 中文启动器打开"
  WriteRegStr HKCU "Software\Classes\Directory\shell\CursorZhLauncher" "Icon" \
    "$CursorIconPath"
  WriteRegStr HKCU "Software\Classes\Directory\shell\CursorZhLauncher\command" "" \
    '"$INSTDIR\cursor-zh-launcher.exe" "%1"'

  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\CursorZhLauncher" "" \
    "在此处打开 Cursor 中文启动器"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\CursorZhLauncher" "Icon" \
    "$CursorIconPath"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\CursorZhLauncher\command" "" \
    '"$INSTDIR\cursor-zh-launcher.exe" "%V"'
SectionEnd

LangString DESC_SecMain ${LANG_SIMPCHINESE} \
  "安装启动器，并创建独立的桌面和开始菜单入口。不会替换官方 Cursor 入口。"
LangString DESC_SecContextMenu ${LANG_SIMPCHINESE} \
  "为文件、文件夹和文件夹背景添加可选的中文启动器右键入口。"

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} $(DESC_SecMain)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecContextMenu} $(DESC_SecContextMenu)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  SetShellVarContext current
  IfFileExists "$INSTDIR\cursor-zh-launcher.exe" 0 +2
    ExecWait '"$INSTDIR\cursor-zh-launcher.exe" --zh-restore'

  DeleteRegKey HKCU "Software\Classes\*\shell\CursorZhLauncher"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\CursorZhLauncher"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\CursorZhLauncher"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "Software\CursorZhLauncher"

  Delete "$DESKTOP\Cursor 中文启动器.lnk"
  Delete "$SMPROGRAMS\Cursor 中文启动器\Cursor 中文启动器.lnk"
  RMDir "$SMPROGRAMS\Cursor 中文启动器"

  Delete "$INSTDIR\cursor-zh-launcher.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  MessageBox MB_YESNO|MB_ICONQUESTION \
    "是否同时删除 Cursor 中文启动器的配置、日志、NLS 覆盖和备份？" /SD IDNO \
    IDNO keep_user_data
  RMDir /r "$LOCALAPPDATA\CursorZhLauncher"
  RMDir /r "$APPDATA\CursorZhLauncher"
keep_user_data:
SectionEnd
