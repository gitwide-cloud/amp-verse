using System;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;

[assembly: AssemblyTitle("AMP Verse Desktop Setup")]
[assembly: AssemblyProduct("AMP Verse")]
[assembly: AssemblyVersion("1.1.0.0")]

// The existing verified PowerShell installer owns installation and rollback.
// This wrapper only verifies/extracts its embedded package and provides a Windows entry point.
internal static class Installer {
    const string PackageSha256 = "@@PACKAGE_SHA256@@";
    const string ScriptSha256 = "@@SCRIPT_SHA256@@";
    const string Build = "@@BUILD@@";
    const string Version = "@@VERSION@@";
    static string Hash(Stream input) { using (var hash = SHA256.Create()) return BitConverter.ToString(hash.ComputeHash(input)).Replace("-", "").ToLowerInvariant(); }
    static string HashFile(string path) { using (var input = File.OpenRead(path)) return Hash(input); }
    static string Quote(string value) {
        if (value.IndexOf('"') >= 0 || value.IndexOf('\0') >= 0) throw new ArgumentException("Invalid installation path.");
        return "\"" + System.Text.RegularExpressions.Regex.Replace(value, @"(\\+)$", "$1$1") + "\"";
    }
    static string Extract() {
        string parent = Path.GetFullPath(Path.GetTempPath());
        string root = Path.Combine(parent, "AMPVerseSetup-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        using (var bytes = Assembly.GetExecutingAssembly().GetManifestResourceStream("amp-package.zip")) {
            if (bytes == null || Hash(bytes) != PackageSha256) throw new IOException("The installer download is damaged. Download it again.");
            bytes.Position = 0;
            using (var zip = new ZipArchive(bytes, ZipArchiveMode.Read)) {
                long total = 0;
                foreach (var entry in zip.Entries) {
                    string relative = entry.FullName.Replace('/', Path.DirectorySeparatorChar);
                    string destination = Path.GetFullPath(Path.Combine(root, relative));
                    int kind = (entry.ExternalAttributes >> 16) & 0xF000;
                    if (Path.IsPathRooted(relative) || relative.Contains(":") || !destination.StartsWith(root + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) || kind == 0xA000)
                        throw new IOException("Invalid embedded installation path.");
                    total += entry.Length;
                    if (total > 2L * 1024 * 1024 * 1024) throw new IOException("Embedded package exceeds its size limit.");
                    if (entry.Name.Length == 0) { Directory.CreateDirectory(destination); continue; }
                    Directory.CreateDirectory(Path.GetDirectoryName(destination));
                    using (var input = entry.Open()) using (var output = new FileStream(destination, FileMode.CreateNew)) input.CopyTo(output);
                }
            }
        }
        if (HashFile(Path.Combine(root, "install-windows-package.ps1")) != ScriptSha256) throw new IOException("Installer script verification failed.");
        return root;
    }
    static async Task<string> Install(string root, string music, string fixture) {
        string arguments = "-NoProfile -ExecutionPolicy Bypass -STA -File " + Quote(Path.Combine(root, "install-windows-package.ps1"))
            + " -PackageRoot " + Quote(root) + " -MusicDirectory " + Quote(music) + " -NoStart";
        arguments += fixture == null ? " -CreateShortcuts" : " -FixtureRoot " + Quote(fixture);
        var info = new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "WindowsPowerShell/v1.0/powershell.exe"), arguments) {
            UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true
        };
        info.EnvironmentVariables["PSModulePath"] = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "WindowsPowerShell/v1.0/Modules");
        using (var child = Process.Start(info)) {
            Task<string> output = child.StandardOutput.ReadToEndAsync(), error = child.StandardError.ReadToEndAsync();
            await Task.Run(() => child.WaitForExit());
            string text = await output, problem = await error;
            File.WriteAllText(Path.Combine(root, "installation.log"), text + Environment.NewLine + problem);
            if (child.ExitCode != 0) throw new IOException("Installation did not finish. " + problem.Trim() + "\n\nLog: " + Path.Combine(root, "installation.log"));
            return text;
        }
    }
    static void Cleanup(string root) {
        if (root == null) return;
        string expected = Path.GetFullPath(Path.GetTempPath());
        string full = Path.GetFullPath(root);
        if (full.StartsWith(expected, StringComparison.OrdinalIgnoreCase) && Path.GetFileName(full).StartsWith("AMPVerseSetup-", StringComparison.Ordinal)) {
            try { Directory.Delete(full, true); } catch (IOException) {} catch (UnauthorizedAccessException) {}
        }
    }
    [STAThread] static int Main(string[] args) {
        try {
            if (!Environment.Is64BitOperatingSystem) throw new PlatformNotSupportedException("This build requires 64-bit Windows 10 or Windows 11.");
            if (args.Length == 1 && args[0] == "--verify-only") {
                string root = Extract(); Cleanup(root); Console.WriteLine("Verified AMP Verse " + Version + " / " + Build); return 0;
            }
            if (args.Length == 4 && args[0] == "--fixture-root" && args[2] == "--music") {
                string root = Extract();
                try { Console.WriteLine(Install(root, args[3], args[1]).GetAwaiter().GetResult()); Cleanup(root); return 0; }
                catch { Console.Error.WriteLine("Retained installer diagnostic folder: " + root); throw; }
            }
            if (args.Length != 0) throw new ArgumentException("Unsupported installer arguments.");
            Application.EnableVisualStyles(); Application.SetCompatibleTextRenderingDefault(false); Application.Run(new SetupForm()); return 0;
        } catch (Exception error) {
            if (args.Length == 0) MessageBox.Show(error.Message, "AMP Verse Setup", MessageBoxButtons.OK, MessageBoxIcon.Error);
            else Console.Error.WriteLine(error.Message);
            return 1;
        }
    }
    sealed class SetupForm : Form {
        readonly TextBox music = new TextBox();
        readonly Label status = new Label();
        readonly Button install = new Button();
        readonly CheckBox launch = new CheckBox();
        bool busy, finished;
        public SetupForm() {
            Text = "AMP Verse Desktop Setup"; ClientSize = new System.Drawing.Size(580, 290);
            FormBorderStyle = FormBorderStyle.FixedDialog; MaximizeBox = false; StartPosition = FormStartPosition.CenterScreen;
            Font = new System.Drawing.Font("Segoe UI", 10);
            var title = new Label { Text = "Install AMP Verse", Left = 20, Top = 18, Width = 530, Height = 36, Font = new System.Drawing.Font("Segoe UI", 19, System.Drawing.FontStyle.Bold) };
            Controls.Add(title);
            Controls.Add(new Label { Text = Version + "  ·  Windows x64  ·  build " + Build.Substring(0, 8), Left = 22, Top = 59, Width = 530 });
            Controls.Add(new Label { Text = "Music folder (local folder or network share)", Left = 22, Top = 94, Width = 530 });
            music.SetBounds(22, 122, 440, 28); music.Text = Environment.GetFolderPath(Environment.SpecialFolder.MyMusic); Controls.Add(music);
            var browse = new Button { Text = "Browse…", Left = 470, Top = 120, Width = 88, Height = 31 };
            browse.Click += (s, e) => { using (var picker = new FolderBrowserDialog { Description = "Choose your music folder. Existing files stay in place.", ShowNewFolderButton = false }) { if (Directory.Exists(music.Text)) picker.SelectedPath = music.Text; if (picker.ShowDialog(this) == DialogResult.OK) music.Text = picker.SelectedPath; } }; Controls.Add(browse);
            launch.Text = "Open AMP Verse when installation finishes"; launch.Checked = true; launch.SetBounds(22, 165, 530, 28); Controls.Add(launch);
            status.SetBounds(22, 198, 530, 40); status.Text = "Installs for this Windows account. Your music files stay in place."; Controls.Add(status);
            install.Text = "Install"; install.SetBounds(440, 242, 118, 34); Controls.Add(install); AcceptButton = install;
            FormClosing += (s, e) => { if (busy) e.Cancel = true; };
            install.Click += async (s, e) => {
                if (finished) { Close(); return; }
                if (!Directory.Exists(music.Text)) { MessageBox.Show(this, "Choose an existing music folder or network share.", Text); return; }
                busy = true; install.Enabled = browse.Enabled = music.Enabled = launch.Enabled = false; UseWaitCursor = true; string root = null;
                try {
                    status.Text = "Verifying and installing…";
                    root = await Task.Run(() => Extract());
                    await Install(root, music.Text, null);
                    Cleanup(root); finished = true; install.Text = "Close"; status.Text = "AMP Verse is installed. A Start menu shortcut is ready.";
                    if (launch.Checked) {
                        string script = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs/AMP Verse/launch-windows-installed.ps1");
                        try { Process.Start(new ProcessStartInfo("powershell.exe", "-NoProfile -ExecutionPolicy Bypass -File " + Quote(script)) { UseShellExecute = false, CreateNoWindow = true }); }
                        catch (Exception openError) { MessageBox.Show(this, "AMP Verse is installed. Open it from the Start menu.\n\n" + openError.Message, Text); }
                    }
                } catch (Exception error) { status.Text = "Installation did not finish. Your existing installation was retained."; MessageBox.Show(this, error.Message, Text, MessageBoxButtons.OK, MessageBoxIcon.Error); }
                finally { busy = false; install.Enabled = true; UseWaitCursor = false; if (!finished) browse.Enabled = music.Enabled = launch.Enabled = true; }
            };
        }
    }
}
