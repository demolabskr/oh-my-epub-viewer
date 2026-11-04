; The installer of EPUB Viewer: installs `nsh package --target windows`'s
; output for the current user (no administrator rights) and registers the
; app for .epub files, so Windows lists it under Default apps.
;
; Build: nsh package --target windows, then
;   ISCC installer\epub-viewer.iss      (writes dist\EPUB-Viewer-Setup-<version>.exe)

#define AppName "EPUB Viewer"
#define AppNameKo "EPUB 뷰어"
#define AppVersion "1.0.2"
#define AppExe "EPUB Viewer.exe"
#define ProgId "OpenLearningLab.EpubViewer.epub"
#define CapKey "Software\OpenLearningLab\EpubViewer\Capabilities"
#define Source "..\.nsh\out\EPUB Viewer"

[Setup]
AppId={{6F1C2E7A-4B8D-4E3A-9C55-0E2B7D9A41C3}
AppName={#AppNameKo}
AppVersion={#AppVersion}
AppVerName={#AppNameKo} {#AppVersion}
AppPublisher=OpenLearningLab
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppNameKo}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ChangesAssociations=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppNameKo}
OutputDir=..\dist
OutputBaseFilename=EPUB-Viewer-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "ko"; MessagesFile: "compiler:Languages\Korean.isl"

[Tasks]
Name: "assoc"; Description: ".epub 파일을 {#AppNameKo}로 열기"; GroupDescription: "파일 연결:"
Name: "desktopicon"; Description: "바탕 화면에 아이콘 만들기"; GroupDescription: "추가 아이콘:"; Flags: unchecked

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppNameKo}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppNameKo}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; The file type the app opens .epub files as.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "EPUB 전자책"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppNameKo}"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
; "Open with" lists the app for .epub.
Root: HKA; Subkey: "Software\Classes\.epub\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppNameKo}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".epub"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
; The default for .epub where the user has not picked another app.
Root: HKA; Subkey: "Software\Classes\.epub"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.epub"; ValueType: string; ValueName: "Content Type"; ValueData: "application/epub+zip"; Tasks: assoc
; Settings > Apps > Default apps lists the app with what it opens.
Root: HKA; Subkey: "{#CapKey}"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppNameKo}"; Flags: uninsdeletekey
Root: HKA; Subkey: "{#CapKey}"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "EPUB 2 / EPUB 3 전자책을 A4 세로 쪽으로 보여 줍니다."
Root: HKA; Subkey: "{#CapKey}"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\{#AppExe}"",0"
Root: HKA; Subkey: "{#CapKey}\FileAssociations"; ValueType: string; ValueName: ".epub"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\OpenLearningLab\EpubViewer"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\OpenLearningLab"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "{#CapKey}"; Flags: uninsdeletevalue

[Run]
Filename: "ms-settings:defaultapps?registeredAppUser={#AppName}"; Description: "Windows 기본 앱 설정 열기 (EPUB 기본 뷰어로 지정)"; Flags: postinstall shellexec skipifsilent nowait
Filename: "{app}\{#AppExe}"; Description: "{#AppNameKo} 실행"; Flags: postinstall nowait skipifsilent unchecked
