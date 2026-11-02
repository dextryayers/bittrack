using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Interactivity;

namespace Veritas.App.Views;

public partial class ConfirmDialog : Window
{
    public ConfirmDialog()
    {
        InitializeComponent();
    }

    public bool Confirmed { get; private set; }

    public static async Task<bool> ShowAsync(
        Window owner,
        string title,
        string message,
        string confirmLabel)
    {
        var dialog = new ConfirmDialog
        {
            Title = title,
        };
        dialog.TitleText.Text = title;
        dialog.MessageText.Text = message;
        dialog.ConfirmButton.Content = confirmLabel;
        await dialog.ShowDialog(owner);
        return dialog.Confirmed;
    }

    private void OnConfirmClick(object? sender, RoutedEventArgs e)
    {
        Confirmed = true;
        Close();
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e)
    {
        Close();
    }
}
