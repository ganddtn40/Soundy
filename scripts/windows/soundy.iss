#define AppName "Soundy"
#define AppPublisher "Soundy"
#define AppExeName "soundy.exe"
#define AppVersion GetEnv("SOUNDY_VERSION")
#define SourceExe GetEnv("SOUNDY_EXE")
#define OutputDir GetEnv("SOUNDY_DIST")
#define BuildMetadata GetEnv("SOUNDY_BUILD_METADATA")
; x64compatible for the x64 build, arm64 for the ARM one
#define Arch GetEnv("SOUNDY_ARCH")
; Soundy-Setup for the x64 build, Soundy-Setup-arm64 for the ARM one
#define SetupName GetEnv("SOUNDY_SETUP")

[Setup]
AppId={{7A3C8E52-9B4F-4D61-9C2A-6F1E8D5B0A37}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#AppExeName}
OutputDir={#OutputDir}
OutputBaseFilename={#SetupName}
SetupIconFile=..\..\assets\windows\soundy.ico
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName} Setup
VersionInfoCopyright=GPL-3.0-or-later. Based on Sonora by its contributors.
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed={#Arch}
ArchitecturesInstallIn64BitMode={#Arch}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
WizardStyle=modern
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
#if BuildMetadata != ""
Source: "{#BuildMetadata}"; DestDir: "{app}"; DestName: "soundy-build.json"; Flags: ignoreversion
#endif
Source: "..\..\COPYING"; DestDir: "{app}"; DestName: "LICENSE"; Flags: ignoreversion
Source: "..\..\THIRD-PARTY.md"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
Type: files; Name: "{app}\soundy-build.json"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon; Check: not SilentUpgrade

; Lists Soundy in "Open With" and in Settings > Apps > Default apps for the file types
; it plays, without automatically becoming the default handler for any of them.
; MultiSelectModel=Player is the key Explorer honors to invoke the app once
; with every selected file passed as its own argument, instead of once per file.
[Registry]
Root: HKA; Subkey: "Software\Soundy"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Soundy\Capabilities"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Soundy\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A lightweight native music player"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\{#AppExeName},0"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\Soundy\Capabilities"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
; .mp3
Root: HKA; Subkey: "Software\Classes\.mp3"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.mp3\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.mp3\OpenWithProgids"; ValueType: string; ValueName: "Soundy.mp3"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.mp3"; ValueType: string; ValueName: ""; ValueData: "MP3 Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.mp3"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.mp3\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.mp3\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".mp3"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mp3"; ValueData: "Soundy.mp3"; Flags: uninsdeletevalue
; .flac
Root: HKA; Subkey: "Software\Classes\.flac"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.flac\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.flac\OpenWithProgids"; ValueType: string; ValueName: "Soundy.flac"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.flac"; ValueType: string; ValueName: ""; ValueData: "FLAC Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.flac"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.flac\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.flac\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".flac"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".flac"; ValueData: "Soundy.flac"; Flags: uninsdeletevalue
; .m4a
Root: HKA; Subkey: "Software\Classes\.m4a"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.m4a\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.m4a\OpenWithProgids"; ValueType: string; ValueName: "Soundy.m4a"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.m4a"; ValueType: string; ValueName: ""; ValueData: "MPEG-4 Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.m4a"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.m4a\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.m4a\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".m4a"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".m4a"; ValueData: "Soundy.m4a"; Flags: uninsdeletevalue
; .mp4 (no default apps entry, since most .mp4 files are videos)
Root: HKA; Subkey: "Software\Classes\.mp4"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.mp4\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.mp4\OpenWithProgids"; ValueType: string; ValueName: "Soundy.mp4"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.mp4"; ValueType: string; ValueName: ""; ValueData: "MPEG-4 Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.mp4"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.mp4\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.mp4\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".mp4"; ValueData: ""; Flags: uninsdeletevalue
; .aac
Root: HKA; Subkey: "Software\Classes\.aac"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aac\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aac\OpenWithProgids"; ValueType: string; ValueName: "Soundy.aac"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.aac"; ValueType: string; ValueName: ""; ValueData: "AAC Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.aac"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.aac\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.aac\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".aac"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aac"; ValueData: "Soundy.aac"; Flags: uninsdeletevalue
; .ogg
Root: HKA; Subkey: "Software\Classes\.ogg"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.ogg\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.ogg\OpenWithProgids"; ValueType: string; ValueName: "Soundy.ogg"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.ogg"; ValueType: string; ValueName: ""; ValueData: "Ogg Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.ogg"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.ogg\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.ogg\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".ogg"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ogg"; ValueData: "Soundy.ogg"; Flags: uninsdeletevalue
; .oga
Root: HKA; Subkey: "Software\Classes\.oga"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.oga\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.oga\OpenWithProgids"; ValueType: string; ValueName: "Soundy.oga"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.oga"; ValueType: string; ValueName: ""; ValueData: "Ogg Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.oga"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.oga\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.oga\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".oga"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".oga"; ValueData: "Soundy.oga"; Flags: uninsdeletevalue
; .opus
Root: HKA; Subkey: "Software\Classes\.opus"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.opus\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.opus\OpenWithProgids"; ValueType: string; ValueName: "Soundy.opus"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.opus"; ValueType: string; ValueName: ""; ValueData: "Opus Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.opus"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.opus\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.opus\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".opus"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".opus"; ValueData: "Soundy.opus"; Flags: uninsdeletevalue
; .wav
Root: HKA; Subkey: "Software\Classes\.wav"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.wav\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.wav\OpenWithProgids"; ValueType: string; ValueName: "Soundy.wav"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.wav"; ValueType: string; ValueName: ""; ValueData: "WAVE Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.wav"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.wav\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.wav\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".wav"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wav"; ValueData: "Soundy.wav"; Flags: uninsdeletevalue
; .aiff
Root: HKA; Subkey: "Software\Classes\.aiff"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aiff\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aiff\OpenWithProgids"; ValueType: string; ValueName: "Soundy.aiff"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.aiff"; ValueType: string; ValueName: ""; ValueData: "AIFF Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.aiff"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.aiff\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.aiff\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".aiff"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aiff"; ValueData: "Soundy.aiff"; Flags: uninsdeletevalue
; .aif
Root: HKA; Subkey: "Software\Classes\.aif"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aif\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aif\OpenWithProgids"; ValueType: string; ValueName: "Soundy.aif"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.aif"; ValueType: string; ValueName: ""; ValueData: "AIFF Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.aif"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.aif\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.aif\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".aif"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aif"; ValueData: "Soundy.aif"; Flags: uninsdeletevalue
; .aifc
Root: HKA; Subkey: "Software\Classes\.aifc"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aifc\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.aifc\OpenWithProgids"; ValueType: string; ValueName: "Soundy.aifc"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.aifc"; ValueType: string; ValueName: ""; ValueData: "AIFF-C Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.aifc"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.aifc\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.aifc\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".aifc"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aifc"; ValueData: "Soundy.aifc"; Flags: uninsdeletevalue
; .webm
Root: HKA; Subkey: "Software\Classes\.webm"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.webm\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.webm\OpenWithProgids"; ValueType: string; ValueName: "Soundy.webm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.webm"; ValueType: string; ValueName: ""; ValueData: "WebM Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.webm"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.webm\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.webm\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".webm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".webm"; ValueData: "Soundy.webm"; Flags: uninsdeletevalue
; .mka
Root: HKA; Subkey: "Software\Classes\.mka"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.mka\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.mka\OpenWithProgids"; ValueType: string; ValueName: "Soundy.mka"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.mka"; ValueType: string; ValueName: ""; ValueData: "Matroska Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.mka"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.mka\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.mka\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".mka"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mka"; ValueData: "Soundy.mka"; Flags: uninsdeletevalue
; .wv
Root: HKA; Subkey: "Software\Classes\.wv"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.wv\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.wv\OpenWithProgids"; ValueType: string; ValueName: "Soundy.wv"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.wv"; ValueType: string; ValueName: ""; ValueData: "WavPack Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.wv"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.wv\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.wv\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".wv"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wv"; ValueData: "Soundy.wv"; Flags: uninsdeletevalue
; .ape
Root: HKA; Subkey: "Software\Classes\.ape"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.ape\OpenWithProgids"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.ape\OpenWithProgids"; ValueType: string; ValueName: "Soundy.ape"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Soundy.ape"; ValueType: string; ValueName: ""; ValueData: "Monkey's Audio"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Soundy.ape"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Player"
Root: HKA; Subkey: "Software\Classes\Soundy.ape\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\Soundy.ape\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".ape"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Soundy\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ape"; ValueData: "Soundy.ape"; Flags: uninsdeletevalue
; soundy, which no other app claims, so Soundy owns the scheme outright
Root: HKA; Subkey: "Software\Classes\soundy"; ValueType: string; ValueName: ""; ValueData: "URL:Soundy Link"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\soundy"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\soundy\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\soundy\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
; spotify
Root: HKA; Subkey: "Software\Soundy\Capabilities\UrlAssociations"; ValueType: string; ValueName: "spotify"; ValueData: "SoundySpotify"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\SoundySpotify"; ValueType: string; ValueName: ""; ValueData: "Spotify Link"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SoundySpotify"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\SoundySpotify\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\SoundySpotify\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
; deezer
Root: HKA; Subkey: "Software\Soundy\Capabilities\UrlAssociations"; ValueType: string; ValueName: "deezer"; ValueData: "SoundyDeezer"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\SoundyDeezer"; ValueType: string; ValueName: ""; ValueData: "Deezer Link"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SoundyDeezer"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\SoundyDeezer\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\SoundyDeezer\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
; tidal
Root: HKA; Subkey: "Software\Soundy\Capabilities\UrlAssociations"; ValueType: string; ValueName: "tidal"; ValueData: "SoundyTidal"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\SoundyTidal"; ValueType: string; ValueName: ""; ValueData: "TIDAL Link"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SoundyTidal"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\SoundyTidal\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKA; Subkey: "Software\Classes\SoundyTidal\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""

; Setup runs elevated, and a [Run] entry inherits that unless it says otherwise: postinstall
; entries default to runasoriginaluser, the relaunch after a silent update does not, and an
; elevated Soundy is out of reach for tools like FancyZones that manage windows unelevated.
[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\{#AppExeName}"; Flags: nowait runasoriginaluser; Check: RelaunchRequested

[Code]
function RelaunchRequested: Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

// A silent run over an existing install is an update from the app or a package manager. It
// leaves the desktop alone, so a shortcut the user deleted or replaced stays that way.
function SilentUpgrade: Boolean;
begin
  Result := WizardSilent and (WizardForm.PrevAppDir <> '');
end;
