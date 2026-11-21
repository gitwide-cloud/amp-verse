using Amp.ServerManager;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

internal sealed class ManagerContext : ApplicationContext
{
    private readonly string profilePath;
    private ServerProfile profile;
    private readonly NotifyIcon tray;
    private readonly System.Windows.Forms.Timer timer = new() { Interval = 5000 };
    private readonly ToolStripMenuItem status = new("Checking server…") { Enabled = false };
    private Form? window;

    public ManagerContext(string path, ServerProfile initial, bool open)
    {
        profilePath = path;
        profile = initial;
        var menu = new ContextMenuStrip();
        menu.Items.Add(status);
        menu.Items.Add("Open server window", null, (_, _) => OpenWindow());
        menu.Items.Add("Manager settings…", null, (_, _) => Settings());
        menu.Items.Add("Start existing installation", null, (_, _) => Start());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Exit manager (server keeps running)", null, (_, _) => ExitThread());
        tray = new NotifyIcon { Icon = SystemIcons.Application, Text = "AMP Verse Server", ContextMenuStrip = menu, Visible = true };
        tray.DoubleClick += (_, _) => OpenWindow();
        timer.Tick += (_, _) => RefreshStatus();
        timer.Start();
        RefreshStatus();
        if (open) OpenWindow();
    }
    private void RefreshStatus()
    {
        try { status.Text = new ServerController(profile, new ProcessHost()).Inspect().State; }
        catch (Exception e) { status.Text = "Cannot inspect server: " + e.Message; }
    }
    private void Start()
    {
        try {
            var pid = new ServerController(profile, new ProcessHost()).Start();
            MessageBox.Show($"Started PID {pid}. Check the server window for readiness. Closing this manager will not stop it.", "AMP Verse Server");
        } catch (Exception e) { MessageBox.Show(e.Message, "Server not started", MessageBoxButtons.OK, MessageBoxIcon.Warning); }
        RefreshStatus();
    }
    private void Settings()
    {
        using var editor = new SettingsForm(profile);
        if (editor.ShowDialog() != DialogResult.OK) return;
        try {
            var updated = editor.Profile;
            if (updated.Identity != profile.Identity)
                throw new InvalidOperationException("To manage another installation, exit this manager and select its profile. The running server was not changed.");
            updated.Save(profilePath);
            profile = updated;
            window?.Close();
            RefreshStatus();
        } catch (Exception e) { MessageBox.Show(e.Message, "Settings not saved"); }
    }
    private async void OpenWindow()
    {
        if (window is not null) { window.Show(); window.Activate(); return; }
        var form = new Form { Text = "AMP Verse Server — close window to keep playback running", Width = 1150, Height = 800, MinimumSize = new Size(640, 480) };
        var browser = new WebView2 { Dock = DockStyle.Fill };
        form.Controls.Add(browser);
        window = form;
        form.FormClosed += (_, _) => { window = null; browser.Dispose(); };
        form.Show();
        try {
            var data = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "AMPVerse", "ServerManager", profile.Identity, "WebView");
            var environment = await CoreWebView2Environment.CreateAsync(null, data);
            if (form.IsDisposed) return;
            await browser.EnsureCoreWebView2Async(environment);
            if (form.IsDisposed) return;
            var origin = new Uri(profile.WebUrl);
            browser.CoreWebView2.Settings.AreDevToolsEnabled = false;
            browser.CoreWebView2.Settings.AreHostObjectsAllowed = false;
            browser.CoreWebView2.Settings.IsWebMessageEnabled = false;
            browser.CoreWebView2.NavigationStarting += (_, e) => {
                if (!Uri.TryCreate(e.Uri, UriKind.Absolute, out var target) || target.GetLeftPart(UriPartial.Authority) != origin.GetLeftPart(UriPartial.Authority)) e.Cancel = true;
            };
            browser.CoreWebView2.NewWindowRequested += (_, e) => e.Handled = true;
            browser.CoreWebView2.PermissionRequested += (_, e) => e.State = CoreWebView2PermissionState.Deny;
            browser.CoreWebView2.Navigate(profile.WebUrl);
        } catch (Exception e) {
            if (!form.IsDisposed) {
                browser.Dispose();
                form.Controls.Add(new Label { Dock = DockStyle.Fill, Padding = new Padding(24), Text = "The embedded server window could not open. Check that Microsoft Edge WebView2 Runtime is installed. The server is unchanged and remains accessible in your browser.\n\n" + e.Message });
            }
        }
    }
    protected override void ExitThreadCore()
    {
        timer.Stop();
        window?.Close();
        tray.Visible = false;
        base.ExitThreadCore();
    }
    protected override void Dispose(bool disposing)
    {
        if (disposing) { timer.Dispose(); tray.Dispose(); window?.Dispose(); }
        base.Dispose(disposing);
    }
}

internal sealed class SettingsForm : Form
{
    private readonly TextBox[] fields;
    public ServerProfile Profile => new(fields[0].Text, fields[1].Text, fields[2].Text, fields[3].Text, fields[4].Text, fields[5].Text);
    public SettingsForm(ServerProfile profile)
    {
        Text = "Manager settings — existing server configuration is preserved";
        Width = 760; Height = 440;
        var panel = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 8, Padding = new Padding(12) };
        panel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 170));
        panel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        fields = new[] { profile.Executable, profile.MusicDirectory, profile.ConfigDirectory, profile.DeviceDirectory, profile.Bind, profile.WebUrl }.Select(v => new TextBox { Text = v, Dock = DockStyle.Fill }).ToArray();
        var names = new[] { "Installed executable", "Existing music folder", "Server config directory", "Device identity directory", "Listen IP:port", "Local server web URL" };
        for (var i = 0; i < fields.Length; i++) { panel.Controls.Add(new Label { Text = names[i], AutoSize = true }, 0, i); panel.Controls.Add(fields[i], 1, i); }
        var note = new Label { Text = "These preferences affect only a future explicit Start. Saving never restarts the server, edits server.json, changes the firewall or stops playback.", AutoSize = true, MaximumSize = new Size(690, 0) };
        panel.Controls.Add(note, 0, 6); panel.SetColumnSpan(note, 2);
        var save = new Button { Text = "Save preferences", AutoSize = true, DialogResult = DialogResult.OK };
        panel.Controls.Add(save, 1, 7);
        Controls.Add(panel); AcceptButton = save;
    }
}
