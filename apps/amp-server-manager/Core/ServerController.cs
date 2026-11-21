using System.Diagnostics;

namespace Amp.ServerManager;

public sealed record ServerProcess(int Id, string? Executable);
public sealed record Inspection(string State, int[] ProcessIds, bool CanStart);

public interface IProcessHost
{
    IReadOnlyList<ServerProcess> FindServers();
    int Start(ProcessStartInfo info);
}

public sealed class ProcessHost : IProcessHost
{
    public IReadOnlyList<ServerProcess> FindServers()
    {
        var result = new List<ServerProcess>();
        foreach (var process in Process.GetProcessesByName("amp-cast"))
        {
            using (process)
            {
                try { result.Add(new(process.Id, process.MainModule?.FileName)); }
                catch (Exception e) when (e is System.ComponentModel.Win32Exception or InvalidOperationException or NotSupportedException)
                { result.Add(new(process.Id, null)); } // Cannot inspect: refuse to launch another server.
            }
        }
        return result;
    }
    public int Start(ProcessStartInfo info)
    {
        using var process = Process.Start(info) ?? throw new IOException("Server process could not start.");
        return process.Id; // Disposing the process handle does not stop the child.
    }
}

public sealed class ServerController(ServerProfile profile, IProcessHost host)
{
    public Inspection Inspect()
    {
        var processes = host.FindServers();
        if (processes.Count == 0) return new("Stopped", [], true);
        var comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        if (processes.Count == 1 && processes[0].Executable is string exe &&
            string.Equals(Path.GetFullPath(exe), Path.GetFullPath(profile.Executable), comparison))
            return new("Existing server process (HTTP health not checked)", [processes[0].Id], false);
        return new("Other, multiple or inaccessible server processes; launch refused", processes.Select(p => p.Id).ToArray(), false);
    }

    public int Start()
    {
        // Machine-wide on Windows: prevent two concurrent manager/CLI launches.
        using var gate = new Mutex(false, (OperatingSystem.IsWindows() ? "Global\\" : "") + "AMPVerseServerStart");
        bool held;
        try { held = gate.WaitOne(TimeSpan.FromSeconds(5)); }
        catch (AbandonedMutexException) { held = true; }
        if (!held) throw new IOException("Another server start is in progress.");
        try {
            var inspection = Inspect();
            if (!inspection.CanStart) throw new InvalidOperationException(inspection.State);
            return host.Start(profile.StartInfo());
        } finally { gate.ReleaseMutex(); }
    }
    // Deliberately no Kill/Stop/Dispose-child path. Server output controls belong
    // to its authenticated web UI; closing the manager must preserve playback.
}
