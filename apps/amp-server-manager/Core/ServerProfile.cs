using System.Diagnostics;
using System.Net;
using System.Net.NetworkInformation;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace Amp.ServerManager;

// Manager preferences are separate from server.json: never rewrite server identity,
// authentication, analysis settings, station state or the shared music catalogue.
public sealed record ServerProfile(string Executable, string MusicDirectory,
    string ConfigDirectory, string DeviceDirectory, string Bind, string WebUrl,
    bool UseInstalledLauncher = false)
{
    public static ServerProfile Load(string path) =>
        JsonSerializer.Deserialize<ServerProfile>(File.ReadAllText(path))
        ?? throw new InvalidDataException("Empty manager profile.");

    public void Validate(bool forStart)
    {
        if (!Path.IsPathFullyQualified(Executable) || !File.Exists(Executable) ||
            !string.Equals(Path.GetFileName(Executable), OperatingSystem.IsWindows() ? "amp-cast.exe" : "amp-cast", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Select the existing installed amp-cast executable.");
        var uri = new Uri(WebUrl, UriKind.Absolute);
        if (uri.Scheme != "http" || !string.IsNullOrEmpty(uri.UserInfo) || !IsLocalAddress(uri.Host))
            throw new InvalidDataException("Server URL must use HTTP and an IP address belonging to this computer; no embedded credentials.");
        if (forStart)
        {
            foreach (var path in new[] { MusicDirectory, ConfigDirectory, DeviceDirectory })
                if (string.IsNullOrWhiteSpace(path) || !Path.IsPathFullyQualified(path) || !Directory.Exists(path))
                    throw new InvalidDataException("Select existing absolute music, server configuration and device identity directories. Nothing was created.");
            if (!File.Exists(Path.Combine(ConfigDirectory, "server.json")) ||
                !File.Exists(Path.Combine(DeviceDirectory, "device.json")))
                throw new InvalidDataException("Existing server.json and device.json are required; initialize a new server explicitly with amp-cast, not the manager.");
            if (!IPEndPoint.TryParse(Bind, out var endpoint) || endpoint.Port == 0 ||
                !(IsLocalAddress(endpoint.Address.ToString()) || endpoint.Address.Equals(IPAddress.Any) || endpoint.Address.Equals(IPAddress.IPv6Any)) ||
                endpoint.Port != uri.Port)
                throw new InvalidDataException("Bind must be a local IP:port and match the web URL port.");
            if (!endpoint.Address.Equals(IPAddress.Any) && !endpoint.Address.Equals(IPAddress.IPv6Any) &&
                !endpoint.Address.Equals(IPAddress.Parse(uri.Host.Trim('[', ']'))))
                throw new InvalidDataException("Web URL must address the selected listener.");
        }
    }

    private static bool IsLocalAddress(string host)
    {
        if (!IPAddress.TryParse(host.Trim('[', ']'), out var ip)) return false;
        if (IPAddress.IsLoopback(ip)) return true;
        return NetworkInterface.GetAllNetworkInterfaces().SelectMany(n => n.GetIPProperties().UnicastAddresses)
            .Any(a => a.Address.Equals(ip));
    }

    public string Identity => Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(
        OperatingSystem.IsWindows() ? Path.GetFullPath(Executable).ToUpperInvariant() : Path.GetFullPath(Executable))));

    public ProcessStartInfo StartInfo()
    {
        Validate(true);
        if (UseInstalledLauncher)
        {
            if (!OperatingSystem.IsWindows())
                throw new InvalidDataException("The installed PowerShell launcher is a Windows option.");
            var directory = Path.GetDirectoryName(Executable)!;
            var launcher = Path.Combine(directory, "Start AMP Verse Server.ps1");
            if (!File.Exists(launcher))
                throw new InvalidDataException("The existing server launcher was not found beside amp-cast.exe.");
            var powershell = Path.Combine(Environment.SystemDirectory, "WindowsPowerShell", "v1.0", "powershell.exe");
            var launch = new ProcessStartInfo(powershell) {
                UseShellExecute = false, CreateNoWindow = true, WorkingDirectory = directory
            };
            foreach (var arg in new[] { "-NoProfile", "-NonInteractive", "-File", launcher })
                launch.ArgumentList.Add(arg);
            return launch;
        }
        var info = new ProcessStartInfo(Executable) {
            UseShellExecute = false, CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(Executable)!
        };
        info.ArgumentList.Add(MusicDirectory);
        info.Environment["AMP_CAST_CONFIG_DIR"] = ConfigDirectory;
        info.Environment["AMP_DEVICE_CONFIG_DIR"] = DeviceDirectory;
        info.Environment["AMP_CAST_BIND"] = Bind;
        // Preserve optional configured credentials/environment; never copy them to a receipt.
        return info;
    }

    public void Save(string path)
    {
        Validate(false);
        var temp = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try {
            File.WriteAllText(temp, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
            File.Move(temp, path, true);
        } finally { if (File.Exists(temp)) File.Delete(temp); }
    }
}
