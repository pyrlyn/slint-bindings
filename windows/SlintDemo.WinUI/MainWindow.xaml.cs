using System;
using System.IO;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SlintBindings.WinUI;

namespace SlintDemo.WinUI;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        // The build copies the repository's demo.slint next to the app; the panel loads after this.
        try
        {
            Slint.Definition = new SlintDefinition(Path.Combine(AppContext.BaseDirectory, "demo.slint"));
        }
        catch (SlintException ex)
        {
            Submissions.Items.Add(ex.Message);
        }
    }

    private void OnSlintLoaded(object sender, RoutedEventArgs e)
    {
        // SlintPanel creates its host in its own Loaded handler, which runs first.
        Slint.Host?.OnCallback("submitted", args =>
            Submissions.Items.Add(JsonSerializer.Deserialize<string[]>(args)?[0] ?? ""));
    }

    private void OnNameChanged(object sender, TextChangedEventArgs e) =>
        Slint.Host?.SetProperty("name", JsonSerializer.Serialize(NameBox.Text));
}
