using System;
using System.IO;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SlintBindings.WinUI;

namespace SlintDemo.WinUI;

public sealed partial class MainWindow : Window
{
    // Set when a .slint path came on the command line, such as fixtures/weft/login.slint.
    private readonly bool _external;

    public MainWindow()
    {
        InitializeComponent();
        var args = Environment.GetCommandLineArgs();
        _external = args.Length > 1;
        // The build copies the repository's demo.slint next to the app; the panel loads after this.
        try
        {
            if (_external)
            {
                var definition = new SlintDefinition(Path.GetFullPath(args[1]));
                // Through the factory, the way a host another library builds reaches the panel.
                Slint.HostFactory = panel => new SlintHost(definition, 1, 1, 1, panel);
            }
            else
            {
                Slint.Definition = new SlintDefinition(Path.Combine(AppContext.BaseDirectory, "demo.slint"));
            }
        }
        catch (SlintException ex)
        {
            Submissions.Items.Add(ex.Message);
        }
    }

    private void OnSlintLoaded(object sender, RoutedEventArgs e)
    {
        // SlintPanel creates its host in its own Loaded handler, which runs first.
        if (Slint.Host is null)
        {
            if (Slint.LastError is { } error) Submissions.Items.Add(error);
            return;
        }
        // A weft screen reports every action through `perform(action, id)`; list its raw arguments.
        if (_external)
        {
            try { Slint.Host.OnCallback("perform", args => Submissions.Items.Add(args)); }
            catch (SlintException ex) { Submissions.Items.Add(ex.Message); }
            return;
        }
        Slint.Host.OnCallback("submitted", args =>
            Submissions.Items.Add(JsonSerializer.Deserialize<string[]>(args)?[0] ?? ""));
    }

    private void OnNameChanged(object sender, TextChangedEventArgs e)
    {
        if (!_external) Slint.Host?.SetProperty("name", JsonSerializer.Serialize(NameBox.Text));
    }
}
