using System.ComponentModel;
using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Xml.Linq;

namespace Twi;

internal static class Sandbox
{
    private const string GuestControl = @"C:\TWIControl";
    private const string GuestSource = @"C:\TWISource";
    internal const string HelpUrl = "https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install";

    internal static bool IsOpen()
    {
        foreach (string name in new[] { "WindowsSandbox", "WindowsSandboxClient", "WindowsSandboxRemoteSession" })
        {
            Process[] processes = Process.GetProcessesByName(name);
            try
            {
                if (processes.Any(process => process.MainWindowHandle != IntPtr.Zero))
                    return true;
            }
            finally
            {
                foreach (Process process in processes)
                    process.Dispose();
            }
        }

        return false;
    }

    internal static void Launch(string file, bool installer, bool networking)
    {
        file = Path.GetFullPath(file);
        string extension = Path.GetExtension(file);
        bool isExe = extension.Equals(".exe", StringComparison.OrdinalIgnoreCase);
        bool isMsi = extension.Equals(".msi", StringComparison.OrdinalIgnoreCase);

        if (!File.Exists(file) || !(isExe || (installer && isMsi)))
            throw new InvalidOperationException("Укажите существующий файл .exe или установщик .msi.");
        if (IsOpen())
            throw new InvalidOperationException("Закройте текущее окно Windows Sandbox и затем запустите новое.");

        string fileName = Path.GetFileName(file);
        string source = Path.GetDirectoryName(file)!;
        if (!installer && string.Equals(source, Path.GetPathRoot(file), StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Выберите программу из её собственной папки, чтобы не открывать песочнице весь диск.");

        string sessions = Path.Combine(License.DataDir, "Sessions");
        Directory.CreateDirectory(sessions);
        DeleteOldSessions(sessions);

        string session = Path.Combine(sessions, Guid.NewGuid().ToString("N"));
        string control = Path.Combine(session, "Control");
        Directory.CreateDirectory(control);

        try
        {
            if (installer)
            {
                source = Path.Combine(session, "Installer");
                Directory.CreateDirectory(source);
                File.Copy(file, Path.Combine(source, fileName));
            }

            File.WriteAllText(Path.Combine(control, "launch.json"),
                JsonSerializer.Serialize(new { fileName, installer, isMsi }));
            File.WriteAllText(Path.Combine(control, "launch.ps1"), LaunchScript, Encoding.Unicode);

            string profilePath = Path.Combine(session, "Two Windows in One.wsb");
            CreateProfile(control, source, networking).Save(profilePath);
            using var process = Process.Start(new ProcessStartInfo(profilePath) { UseShellExecute = true });
        }
        catch (Win32Exception ex) when (ex.NativeErrorCode is 2 or 3 or 1155)
        {
            throw new InvalidOperationException("Windows Sandbox не найдена. Включите компонент «Песочница Windows» и перезагрузите ПК.", ex);
        }
    }

    private static XDocument CreateProfile(string control, string source, bool networking)
    {
        static XElement Map(string host, string guest) => new("MappedFolder",
            new XElement("HostFolder", host),
            new XElement("SandboxFolder", guest),
            new XElement("ReadOnly", true));

        return new XDocument(new XElement("Configuration",
            new XElement("Networking", networking ? "Enable" : "Disable"),
            new XElement("ClipboardRedirection", "Disable"),
            new XElement("MappedFolders", Map(control, GuestControl), Map(source, GuestSource)),
            new XElement("LogonCommand",
                new XElement("Command", $@"powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File {GuestControl}\launch.ps1"))));
    }

    private static void DeleteOldSessions(string sessions)
    {
        foreach (string directory in Directory.EnumerateDirectories(sessions))
        {
            if (!Guid.TryParseExact(Path.GetFileName(directory), "N", out _))
                continue;
            try
            {
                if (Directory.GetCreationTimeUtc(directory) < DateTime.UtcNow.AddDays(-7))
                    Directory.Delete(directory, recursive: true);
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }

    private const string LaunchScript = """
        $ErrorActionPreference = 'Stop'
        Add-Type -AssemblyName PresentationFramework
        try {
            $options = Get-Content -LiteralPath 'C:\TWIControl\launch.json' -Raw -Encoding UTF8 | ConvertFrom-Json
            $source = 'C:\TWISource'
            $target = Join-Path $env:LOCALAPPDATA 'TwoWindowsInOne\App'
            New-Item -ItemType Directory -Path $target -Force | Out-Null
            $file = Join-Path $target $options.fileName

            if ($options.installer) {
                Copy-Item -LiteralPath (Join-Path $source $options.fileName) -Destination $file
            } else {
                & robocopy.exe $source $target /E /XJ /R:0 /W:0 /NFL /NDL /NJH /NJS /NP
                if ($LASTEXITCODE -ge 8) { throw "Не удалось скопировать папку программы (код $LASTEXITCODE)." }
            }

            if ($options.isMsi) {
                Start-Process msiexec.exe -ArgumentList ('/i "' + $file + '"') -WorkingDirectory $target
            } else {
                Start-Process -FilePath $file -WorkingDirectory $target
            }
        } catch {
            [System.Windows.MessageBox]::Show($_.Exception.Message, 'Two Windows in One') | Out-Null
        }
        """;
}
