using Amp.ServerManager;
using System.Text.Json;

internal static class Program
{
    [STAThread]
    static int Main(string[] args)
    {
        if (args.Length != 2 || args[0] is not ("--tray" or "--window" or "--status" or "--start" or "--configure")) {
            Console.Error.WriteLine("amp-server-manager --tray|--window|--status|--start|--configure ABSOLUTE_MANAGER_PROFILE.json\nClosing the manager never stops the server. amp-cast remains usable headlessly.");
            return 2;
        }
        try {
            var path = Path.GetFullPath(args[1]);
            if (args[0] == "--configure") {
                ApplicationConfiguration.Initialize();
                using var editor = new SettingsForm(File.Exists(path) ? ServerProfile.Load(path) :
                    new ServerProfile("", "", "", "", "127.0.0.1:8787", "http://127.0.0.1:8787/#cast"));
                if (editor.ShowDialog() == DialogResult.OK) editor.Profile.Save(path);
                return 0;
            }
            var profile = ServerProfile.Load(path);
            profile.Validate(false);
            var controller = new ServerController(profile, new ProcessHost());
            if (args[0] == "--status") { Console.WriteLine(JsonSerializer.Serialize(controller.Inspect())); return 0; }
            if (args[0] == "--start") { Console.WriteLine($"Started server PID {controller.Start()}; readiness must be checked in the web UI."); return 0; }
            using var singleton = new Mutex(true, "Local\\AMPVerseManager-" + profile.Identity, out var created);
            if (!created) { Console.Error.WriteLine("This installation's manager is already open in the notification area."); return 3; }
            ApplicationConfiguration.Initialize();
            using var context = new ManagerContext(path, profile, args[0] == "--window");
            Application.Run(context);
            singleton.ReleaseMutex();
            return 0;
        } catch (Exception e) {
            Console.Error.WriteLine(e.Message);
            return 1;
        }
    }
}
