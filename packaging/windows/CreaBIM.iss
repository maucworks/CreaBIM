; CreaBIM's Windows setup, built with Inno Setup 6 (free; on GitHub's
; Windows runners already). It installs for the current user, with no
; administrator rights, into %LOCALAPPDATA%\Programs\CreaBIM, makes Start
; menu and desktop shortcuts, adds an entry in Settings > Apps and starts
; CreaBIM at the end. No script host runs at install or uninstall: the
; earlier IExpress setup ran a hidden PowerShell script, which antivirus
; heuristics took for malware (issue #53).
;
; Built by make-setup.ps1, which passes the version, the release date, the
; folder of staged files and the output folder:
;   ISCC /DVersion=1.1.2 /DReleaseDate=2026-11-18 /DStage=... /DOut=... CreaBIM.iss

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef ReleaseDate
  #define ReleaseDate "0000-00-00"
#endif
#ifndef Stage
  #error Pass /DStage=<folder of staged files>
#endif
#ifndef Out
  #define Out "."
#endif

[Setup]
; never change: it ties every version's install and uninstall together
AppId={{4B991CF7-DFDF-4F4F-9775-4C35037B3579}
AppName=CreaBIM
AppVersion={#Version}
AppVerName=CreaBIM {#Version}
AppPublisher=Regis Benoit Brice Nde Tene
AppPublisherURL=https://github.com/chopinregis/CreaBIM
AppSupportURL=https://github.com/chopinregis/CreaBIM/issues
AppUpdatesURL=https://github.com/chopinregis/CreaBIM/releases
VersionInfoVersion={#Version}
VersionInfoDescription=CreaBIM setup
; for the current user, in the same folder the earlier setup used
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\CreaBIM
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
UsePreviousAppDir=no
LicenseFile={#Stage}\LICENSE
OutputDir={#Out}
OutputBaseFilename=CreaBIM-{#Version}-{#ReleaseDate}-windows-x64-setup
SetupIconFile={#Stage}\creabim.ico
UninstallDisplayIcon={app}\CreaBIM.exe
UninstallDisplayName=CreaBIM
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; an open CreaBIM is closed for the update and started again after
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#Stage}\CreaBIM.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\creabim-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\READ ME FIRST.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\VERSION.txt"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; what the earlier setup left that this one no longer uses
Type: files; Name: "{app}\install.ps1"
Type: files; Name: "{app}\uninstall.ps1"

[Registry]
; the earlier setup's Settings > Apps entry, so CreaBIM isn't listed twice
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Uninstall\CreaBIM"; Flags: deletekey

[Icons]
Name: "{userprograms}\CreaBIM"; Filename: "{app}\CreaBIM.exe"; WorkingDir: "{app}"; Comment: "CreaBIM, open-source BIM modeller"
Name: "{userdesktop}\CreaBIM"; Filename: "{app}\CreaBIM.exe"; WorkingDir: "{app}"; Comment: "CreaBIM, open-source BIM modeller"

[Run]
Filename: "{app}\CreaBIM.exe"; Description: "Start CreaBIM"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; the program folder only; projects saved elsewhere are never touched
Type: dirifempty; Name: "{app}"
