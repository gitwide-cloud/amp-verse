using Amp.ServerManager;
using System.Diagnostics;

// A disposable real-process fixture, never an installed server.
if (args.Length == 1 && Directory.Exists(args[0]) && File.Exists(Path.Combine(args[0], "fixture-child"))) {
    File.WriteAllText(Path.Combine(args[0], "child.pid"), Environment.ProcessId.ToString());
    var deadline = DateTime.UtcNow.AddSeconds(30);
    while (!File.Exists(Path.Combine(args[0], "stop-child")) && DateTime.UtcNow < deadline) Thread.Sleep(50);
    return;
}
if (args.Length == 2 && args[0] == "--fixture-launcher") {
    var fixtureProfile = ServerProfile.Load(args[1]);
    new ServerController(fixtureProfile, new FixtureProcessHost(fixtureProfile.Executable)).Start();
    return;
}

var root = Path.Combine(Path.GetTempPath(), "amp-manager-test-" + Guid.NewGuid());
Directory.CreateDirectory(root);
try {
    var exe = Path.Combine(root, OperatingSystem.IsWindows() ? "amp-cast.exe" : "amp-cast");
    File.WriteAllText(exe, "synthetic, never executed");
    File.WriteAllText(Path.Combine(root, "server.json"), "existing config");
    File.WriteAllText(Path.Combine(root, "device.json"), "existing identity");
    var profile = new ServerProfile(exe, root, root, root, "127.0.0.1:8787", "http://127.0.0.1:8787/#cast");
    var host = new FakeHost();
    var controller = new ServerController(profile, host);
    Check(controller.Inspect().CanStart, "empty host can start");
    Check(controller.Start() == 123 && host.Starts == 1, "explicit start exactly once");
    Check(host.Info!.ArgumentList.Single() == root, "music argument is not shell expanded");
    Check(host.Info.Environment["AMP_CAST_CONFIG_DIR"] == root && host.Info.Environment["AMP_DEVICE_CONFIG_DIR"] == root, "original config/identity paths passed");
    Check(!host.Info.UseShellExecute && !host.Info.RedirectStandardOutput, "independent child, no shell or parent output pipe");
    host.Servers = [new(123, exe)];
    Check(!controller.Inspect().CanStart, "existing server detected");
    Reject(() => controller.Start(), "duplicate start");
    host.Servers = [new(456, null)];
    Reject(() => controller.Start(), "inaccessible process fails closed");
    host.Servers = [new(456, Path.Combine(root, "other", Path.GetFileName(exe)))];
    Reject(() => controller.Start(), "different installation not duplicated");
    host.Servers = [new(123, exe), new(456, exe)];
    Reject(() => controller.Start(), "multiple instances not adopted");
    Check(host.Starts == 1, "all refused starts leave process untouched");
    Reject(() => (profile with { WebUrl = "http://example.com/" }).Validate(false), "remote URL refused");
    Reject(() => (profile with { WebUrl = "http://user:secret@127.0.0.1:8787/" }).Validate(false), "embedded credentials refused");
    Reject(() => (profile with { Bind = "127.0.0.1:8788" }).StartInfo(), "mismatched port refused");
    Reject(() => (profile with { ConfigDirectory = Path.Combine(root, "absent") }).StartInfo(), "no new server identity directory");
    var settings = Path.Combine(root, "manager.json");
    var state = Path.Combine(root, "server.json");
    File.WriteAllText(state, "existing profile sentinel");
    profile.Save(settings);
    Check(ServerProfile.Load(settings) == profile, "manager preferences round trip");
    Check(File.ReadAllText(state) == "existing profile sentinel", "server config untouched");
    // Manager launcher exits, while a genuine child keeps running. No network or music.
    var sourceDirectory = AppContext.BaseDirectory;
    foreach (var file in Directory.GetFiles(sourceDirectory)) File.Copy(file, Path.Combine(root, Path.GetFileName(file)), true);
    var appHost = Path.Combine(sourceDirectory, OperatingSystem.IsWindows() ? "Tests.exe" : "Tests");
    File.Copy(appHost, exe, true);
    File.WriteAllText(Path.Combine(root, "fixture-child"), "synthetic");
    profile.Save(settings);
    var launcherInfo = new ProcessStartInfo(appHost) { UseShellExecute = false };
    launcherInfo.ArgumentList.Add("--fixture-launcher"); launcherInfo.ArgumentList.Add(settings);
    using (var launcher = Process.Start(launcherInfo)!) {
        Check(launcher.WaitForExit(10000) && launcher.ExitCode == 0, "fixture manager exits normally");
    }
    var pidPath = Path.Combine(root, "child.pid");
    var until = DateTime.UtcNow.AddSeconds(10);
    while (!File.Exists(pidPath) && DateTime.UtcNow < until) Thread.Sleep(50);
    Check(File.Exists(pidPath), "independent child ready after manager exit");
    using (var child = Process.GetProcessById(int.Parse(File.ReadAllText(pidPath)))) {
        try {
            Thread.Sleep(200);
            Check(!child.HasExited, "child survives manager exit");
            Check(new ProcessHost().FindServers().Any(p => p.Id == child.Id), "real process detection finds fixture");
        } finally {
            File.WriteAllText(Path.Combine(root, "stop-child"), "stop disposable fixture only");
            Check(child.WaitForExit(10000), "fixture child exits cleanly");
        }
    }
    Console.WriteLine("PASS: process ownership, fail-closed detection, launch isolation, path/profile preservation and settings persistence.");
} finally { Directory.Delete(root, true); }

static void Check(bool value, string name) { if (!value) throw new Exception(name); }
static void Reject(Action action, string name) {
    try { action(); } catch (Exception e) when (e is InvalidOperationException or InvalidDataException) { return; }
    throw new Exception("Expected refusal: " + name);
}
sealed class FakeHost : IProcessHost {
    public IReadOnlyList<ServerProcess> Servers = [];
    public int Starts;
    public ProcessStartInfo? Info;
    public IReadOnlyList<ServerProcess> FindServers() => Servers;
    public int Start(ProcessStartInfo info) { Info = info; Starts++; return 123; }
}

// Filter ONLY our disposable fixture. Production ProcessHost remains conservative
// about every amp-cast process; a real server on the test host is never touched.
sealed class FixtureProcessHost(string executable) : IProcessHost {
    public IReadOnlyList<ServerProcess> FindServers() => new ProcessHost().FindServers().Where(p => p.Executable == executable).ToArray();
    public int Start(ProcessStartInfo info) => new ProcessHost().Start(info);
}
