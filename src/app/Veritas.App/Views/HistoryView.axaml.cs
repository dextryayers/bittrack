using Avalonia.Controls;
using Avalonia.Interactivity;
using Veritas.App.ViewModels;

namespace Veritas.App.Views;

public partial class HistoryView : UserControl
{
    public HistoryView()
    {
        InitializeComponent();
    }

    private async void OnClearHistoryClick(object? sender, RoutedEventArgs e)
    {
        if (DataContext is not MainViewModel viewModel || !viewModel.HasHistory)
        {
            return;
        }
        if (TopLevel.GetTopLevel(this) is not Window owner)
        {
            return;
        }

        bool confirmed = await ConfirmDialog.ShowAsync(
            owner,
            "Hapus seluruh riwayat?",
            "Semua catatan analisis beserta hasil lengkapnya akan dihapus dari perangkat ini. " +
            "Tindakan ini tidak dapat dibatalkan.",
            "Hapus semua");
        if (confirmed)
        {
            viewModel.ClearHistoryCommand.Execute(null);
        }
    }
}
