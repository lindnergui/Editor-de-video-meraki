#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

#ifndef BundleDir
  #define BundleDir "..\..\dist\windows\meraki"
#endif

#ifndef OutputDir
  #define OutputDir "..\..\dist\windows"
#endif

#define AppName "Meraki"
#define AppPublisher "Meraki Contributors"
#define AppURL "https://github.com/lindnergui/Editor-de-video-meraki"
#define AppExeName "meraki.exe"

[Setup]
AppId={{9B4A4A53-8E51-4C5E-A289-4F0E5A93D1A0}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE-MIT
OutputDir={#OutputDir}
OutputBaseFilename=Meraki-Setup-{#AppVersion}-x64
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ChangesAssociations=yes
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName} {#AppVersion}

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "assocmeraki"; Description: "Associar projetos .meraki ao Meraki"; GroupDescription: "Integração com o Windows"

[Files]
Source: "{#BundleDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.meraki\OpenWithProgids"; ValueType: string; ValueName: "Meraki.Project"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assocmeraki
Root: HKA; Subkey: "Software\Classes\Meraki.Project"; ValueType: string; ValueName: ""; ValueData: "Projeto do Meraki Video Editor"; Flags: uninsdeletekey; Tasks: assocmeraki
Root: HKA; Subkey: "Software\Classes\Meraki.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"; Tasks: assocmeraki
Root: HKA; Subkey: "Software\Classes\Meraki.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""; Tasks: assocmeraki

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
