using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;

namespace Twi;

internal sealed class MainForm : Form
{
    private static readonly Color Background = Color.FromArgb(16, 21, 20);
    private static readonly Color Surface = Color.FromArgb(25, 34, 30);
    private static readonly Color Field = Color.FromArgb(33, 45, 39);
    private static readonly Color Border = Color.FromArgb(53, 71, 62);
    private static readonly Color TextColor = Color.FromArgb(239, 247, 240);
    private static readonly Color Muted = Color.FromArgb(178, 197, 183);
    private static readonly Color Accent = Color.FromArgb(185, 247, 94);
    private static readonly Color Error = Color.FromArgb(255, 179, 166);

    private readonly ComboBox applications = new() { DropDownStyle = ComboBoxStyle.DropDownList };
    private readonly TextBox keyInput = new() { MaxLength = 512 };
    private readonly RadioButton portable = new() { Text = "Файл программы", Checked = true };
    private readonly RadioButton installer = new() { Text = "Установщик" };
    private readonly CheckBox networking = new() { Text = "Интернет в песочнице" };
    private readonly Label selection;
    private readonly Label keyStatus;
    private readonly Label footer;
    private readonly Button launch;
    private readonly System.Windows.Forms.Timer clock = new() { Interval = 15_000 };

    private string? selectedPath;
    private string activeKey = "";
    private DateTimeOffset expiresAt;
    private bool launching;

    internal MainForm()
    {
        Text = "Two Windows in One";
        BackColor = Background;
        ForeColor = TextColor;
        Font = new Font("Segoe UI", 10);
        AutoScaleMode = AutoScaleMode.Dpi;
        ClientSize = new Size(980, 645);
        MinimumSize = new Size(850, 673);
        StartPosition = FormStartPosition.CenterScreen;
        using (var icon = typeof(MainForm).Assembly.GetManifestResourceStream("Twi.Icon")!)
            Icon = new Icon(icon);

        var root = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(26, 18, 26, 14) };
        root.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        root.RowStyles.Add(new RowStyle(SizeType.Absolute, 53));
        root.RowStyles.Add(new RowStyle(SizeType.Absolute, 102));
        root.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        root.RowStyles.Add(new RowStyle(SizeType.Absolute, 40));
        Controls.Add(root);

        var header = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 3, Margin = Padding.Empty };
        header.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 51));
        header.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        header.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 138));
        using (var logo = typeof(MainForm).Assembly.GetManifestResourceStream("Twi.Logo")!)
        using (var image = Image.FromStream(logo))
            header.Controls.Add(new PictureBox { Image = new Bitmap(image), SizeMode = PictureBoxSizeMode.Zoom,
                Size = new Size(39, 39), Anchor = AnchorStyles.Left, Margin = Padding.Empty }, 0, 0);
        header.Controls.Add(Label("Two Windows in One", 13, TextColor, true), 1, 0);
        header.Controls.Add(Action("Настройка Windows", () => OpenUrl(Sandbox.HelpUrl)), 2, 0);
        root.Controls.Add(header, 0, 0);

        var hero = Column(64, 31);
        hero.Controls.Add(Label("Второе окно, отдельные данные", 24, TextColor, true));
        hero.Controls.Add(Label("Откройте ещё один экземпляр приложения в Windows Sandbox.", 11, Muted));
        root.Controls.Add(hero, 0, 1);

        var cards = new TableLayoutPanel { Dock = DockStyle.Fill, Margin = Padding.Empty, ColumnCount = 3 };
        cards.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 60));
        cards.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 16));
        cards.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 40));
        root.Controls.Add(cards, 0, 2);

        var appCard = Column(45, 24, 44, 30, 39, 38, 31, 42, 48);
        appCard.BackColor = Surface;
        appCard.Padding = new Padding(17, 9, 17, 12);
        appCard.Controls.Add(Label("Приложение", 17, TextColor, true));
        appCard.Controls.Add(Label("Открытые приложения", 9, Muted));
        applications.Dock = DockStyle.Fill;
        applications.BackColor = Field;
        applications.ForeColor = TextColor;
        applications.AccessibleName = "Открытые приложения";
        applications.SelectedIndexChanged += (_, _) =>
        {
            if (applications.SelectedItem is AppEntry entry)
            {
                portable.Checked = true;
                SelectFile(entry.Path);
            }
        };
        var appPicker = new TableLayoutPanel { Dock = DockStyle.Fill, Margin = Padding.Empty, ColumnCount = 2 };
        appPicker.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        appPicker.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 99));
        appPicker.Controls.Add(applications, 0, 0);
        var refresh = Action("Обновить", RefreshApplications);
        refresh.Margin = new Padding(5, 3, 0, 9);
        appPicker.Controls.Add(refresh, 1, 0);
        appCard.Controls.Add(appPicker);
        appCard.Controls.Add(Label("Или выберите файл на диске", 9, Muted));
        var modes = new FlowLayoutPanel { Dock = DockStyle.Fill, Margin = Padding.Empty, WrapContents = false };
        portable.AutoSize = installer.AutoSize = true;
        modes.Controls.Add(portable);
        modes.Controls.Add(installer);
        portable.CheckedChanged += (_, _) =>
        {
            if (portable.Checked && selectedPath is not null &&
                Path.GetExtension(selectedPath).Equals(".msi", StringComparison.OrdinalIgnoreCase))
                SelectFile(null);
        };
        appCard.Controls.Add(modes);
        appCard.Controls.Add(Action("Выбрать файл…", Browse));
        selection = Label("Файл пока не выбран", 9, Muted);
        selection.AutoEllipsis = true;
        appCard.Controls.Add(selection);
        networking.Dock = DockStyle.Fill;
        appCard.Controls.Add(networking);
        launch = Action("Открыть второе окно", async () => await Launch(), primary: true);
        appCard.Controls.Add(launch);
        cards.Controls.Add(appCard, 0, 0);

        var keyCard = Column(45, 54, 33, 49, 62, 40, 56);
        keyCard.BackColor = Surface;
        keyCard.Padding = new Padding(17, 9, 17, 12);
        keyCard.Controls.Add(Label("Ключ доступа", 17, TextColor, true));
        keyCard.Controls.Add(Label("Ключ обновляется каждый день в 00:00 по Москве.", 10, Muted));
        keyInput.Dock = DockStyle.Fill;
        keyInput.BackColor = Field;
        keyInput.ForeColor = TextColor;
        keyInput.BorderStyle = BorderStyle.FixedSingle;
        keyInput.PlaceholderText = "Вставьте ключ с сайта";
        keyInput.AccessibleName = "Ключ доступа";
        keyInput.KeyDown += (_, args) =>
        {
            if (args.KeyCode == Keys.Enter)
            {
                ActivateKey();
                args.SuppressKeyPress = true;
            }
        };
        keyCard.Controls.Add(keyInput);
        keyCard.Controls.Add(Action("Активировать ключ", ActivateKey, primary: true));
        keyStatus = Label("", 10, Muted);
        keyCard.Controls.Add(keyStatus);
        keyCard.Controls.Add(Action("Получить ключ на сайте", () => OpenUrl(ReadSiteUrl())));
        keyCard.Controls.Add(Label("Файлы второго окна удаляются при закрытии песочницы.", 9, Muted));
        cards.Controls.Add(keyCard, 2, 0);

        footer = Label("Windows Sandbox · Windows Pro, Enterprise и Education", 9, Muted);
        footer.AccessibleName = "Статус запуска";
        root.Controls.Add(footer, 0, 3);

        RefreshApplications();
        activeKey = License.Load();
        keyInput.Text = activeKey;
        RefreshLicense();
        clock.Tick += (_, _) => RefreshLicense();
        clock.Start();
        FormClosed += (_, _) => clock.Dispose();
    }

    private static TableLayoutPanel Column(params int[] heights)
    {
        var panel = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 1, Margin = Padding.Empty };
        panel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        foreach (int height in heights)
            panel.RowStyles.Add(new RowStyle(SizeType.Absolute, height));
        return panel;
    }

    private static Label Label(string text, int size, Color color, bool bold = false) => new()
    {
        Text = text, Font = new Font("Segoe UI", size, bold ? FontStyle.Bold : FontStyle.Regular),
        ForeColor = color, TextAlign = ContentAlignment.MiddleLeft, Dock = DockStyle.Fill, Margin = Padding.Empty
    };

    private static Button Action(string text, Action action, bool primary = false)
    {
        var button = new Button
        {
            Text = text, Dock = DockStyle.Fill, FlatStyle = FlatStyle.Flat, Cursor = Cursors.Hand,
            BackColor = primary ? Accent : Field, ForeColor = primary ? Background : TextColor,
            Font = new Font("Segoe UI", 9, FontStyle.Bold), Margin = new Padding(0, 3, 0, 5)
        };
        button.FlatAppearance.BorderColor = primary ? Accent : Border;
        button.FlatAppearance.MouseOverBackColor = primary ? Color.FromArgb(204, 255, 143) : Border;
        button.Click += (_, _) => action();
        button.EnabledChanged += (_, _) =>
        {
            button.BackColor = primary && button.Enabled ? Accent : Field;
            button.FlatAppearance.BorderColor = primary && button.Enabled ? Accent : Border;
        };
        return button;
    }

    private static string ReadSiteUrl()
    {
        using var stream = typeof(MainForm).Assembly.GetManifestResourceStream("Twi.SiteUrl");
        using var reader = stream is null ? null : new StreamReader(stream);
        return reader?.ReadToEnd().Trim() ?? "https://qqew666.github.io/two-windows-in-one/";
    }

    private void OpenUrl(string url)
    {
        try { using var process = Process.Start(new ProcessStartInfo(url) { UseShellExecute = true }); }
        catch (Win32Exception ex) { MessageBox.Show(this, ex.Message, Text, MessageBoxButtons.OK, MessageBoxIcon.Warning); }
    }

    private void RefreshApplications()
    {
        var found = new Dictionary<string, AppEntry>(StringComparer.OrdinalIgnoreCase);
        foreach (Process process in Process.GetProcesses())
        {
            using (process)
            {
                try
                {
                    if (process.Id == Environment.ProcessId || process.MainWindowHandle == IntPtr.Zero)
                        continue;
                    string? path = process.MainModule?.FileName;
                    if (path is null || !Path.GetExtension(path).Equals(".exe", StringComparison.OrdinalIgnoreCase))
                        continue;
                    var info = FileVersionInfo.GetVersionInfo(path);
                    string name = string.IsNullOrWhiteSpace(info.FileDescription)
                        ? Path.GetFileNameWithoutExtension(path) : info.FileDescription;
                    found.TryAdd(path, new AppEntry(name, path));
                }
                catch (Win32Exception) { }
                catch (InvalidOperationException) { }
                catch (IOException) { }
            }
        }

        applications.BeginUpdate();
        applications.Items.Clear();
        applications.Items.Add(found.Count == 0 ? "Нет доступных окон" : "Выберите из списка");
        applications.Items.AddRange(found.Values.OrderBy(app => app.Name).ToArray());
        applications.SelectedIndex = 0;
        applications.EndUpdate();
    }

    private void Browse()
    {
        using var dialog = new OpenFileDialog
        {
            Filter = installer.Checked ? "Установщики Windows (*.exe;*.msi)|*.exe;*.msi" : "Приложения Windows (*.exe)|*.exe",
            Title = installer.Checked ? "Выберите установщик" : "Выберите файл программы",
            CheckFileExists = true
        };
        if (dialog.ShowDialog(this) != DialogResult.OK)
            return;
        applications.SelectedIndex = 0;
        SelectFile(dialog.FileName);
    }

    private void SelectFile(string? path)
    {
        selectedPath = path;
        selection.Text = path is null ? "Файл пока не выбран" : Path.GetFileName(path);
        selection.ForeColor = path is null ? Muted : TextColor;
        RefreshLicense();
    }

    private void ActivateKey()
    {
        string candidate = keyInput.Text.Trim();
        if (!License.Verify(candidate, DateTimeOffset.UtcNow, out _, out string error))
        {
            keyStatus.Text = error;
            keyStatus.ForeColor = Error;
            return;
        }

        activeKey = candidate;
        RefreshLicense();
        try { License.Save(candidate); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            MessageBox.Show(this, "Ключ действует до закрытия приложения. Windows не смогла сохранить его на диск.",
                Text, MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
    }

    private void RefreshLicense()
    {
        bool valid = License.Verify(activeKey, DateTimeOffset.UtcNow, out expiresAt, out string error);
        keyStatus.Text = valid
            ? $"Активен до {expiresAt.ToString("dd MMMM, HH:mm", CultureInfo.GetCultureInfo("ru-RU"))} мск."
            : activeKey.Length == 0 ? "Активируйте ключ перед запуском." : error;
        keyStatus.ForeColor = valid ? Accent : Muted;
        launch.Enabled = valid && selectedPath is not null && !launching;
    }

    private async Task Launch()
    {
        if (launching || selectedPath is null || !License.Verify(activeKey, DateTimeOffset.UtcNow, out _, out _))
        {
            RefreshLicense();
            return;
        }

        string path = selectedPath;
        bool runInstaller = installer.Checked;
        bool allowNetwork = networking.Checked;
        if (!runInstaller)
        {
            DialogResult consent = MessageBox.Show(this,
                $"Windows Sandbox получит доступ только на чтение к папке:\n\n{Path.GetDirectoryName(path)}\n\n" +
                "Её содержимое будет скопировано внутрь песочницы. Продолжить?",
                Text, MessageBoxButtons.OKCancel, MessageBoxIcon.Information);
            if (consent != DialogResult.OK)
                return;
        }

        if (!License.Verify(activeKey, DateTimeOffset.UtcNow, out _, out _))
        {
            RefreshLicense();
            return;
        }

        launching = true;
        RefreshLicense();
        footer.Text = "Подготовка Windows Sandbox…";
        footer.ForeColor = Muted;
        try
        {
            await Task.Run(() => Sandbox.Launch(path, runInstaller, allowNetwork));
            footer.Text = runInstaller ? "Откройте установленную программу внутри Sandbox."
                : "Запрос на запуск отправлен в Windows Sandbox.";
            footer.ForeColor = Accent;
        }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or IOException
                                   or UnauthorizedAccessException or Win32Exception)
        {
            footer.Text = "Не удалось открыть Windows Sandbox.";
            footer.ForeColor = Error;
            MessageBox.Show(this, ex.Message, Text, MessageBoxButtons.OK, MessageBoxIcon.Warning);
        }
        finally
        {
            launching = false;
            RefreshLicense();
        }
    }

    private sealed record AppEntry(string Name, string Path)
    {
        public override string ToString() => Name;
    }
}
