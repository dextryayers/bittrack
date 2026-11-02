using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using Veritas.App.ViewModels;

namespace Veritas.App.Views;

public partial class SettingsView : UserControl
{
    private IBrush? _dropZoneBorder;

    public SettingsView()
    {
        InitializeComponent();
        DragDrop.SetAllowDrop(ModelDropZone, true);
        ModelDropZone.AddHandler(DragDrop.DragEnterEvent, OnModelDragEnter);
        ModelDropZone.AddHandler(DragDrop.DragLeaveEvent, OnModelDragLeave);
        ModelDropZone.AddHandler(DragDrop.DragOverEvent, OnModelDragOver);
        ModelDropZone.AddHandler(DragDrop.DropEvent, OnModelDrop);
    }

    private void OnModelDragEnter(object? sender, DragEventArgs e) => MarkDropTarget(e);

    private void OnModelDragOver(object? sender, DragEventArgs e) => MarkDropTarget(e);

    private void OnModelDragLeave(object? sender, DragEventArgs e) => ResetDropTarget();

    private void MarkDropTarget(DragEventArgs e)
    {
        bool hasFile = e.DataTransfer.Contains(DataFormat.File);
        e.DragEffects = hasFile ? DragDropEffects.Copy : DragDropEffects.None;
        e.Handled = true;
        if (hasFile && _dropZoneBorder is null)
        {
            _dropZoneBorder = ModelDropZone.BorderBrush;
            ModelDropZone.BorderBrush = (IBrush?)Application.Current?
                .FindResource("AccentBrush");
        }
    }

    private void ResetDropTarget()
    {
        if (_dropZoneBorder is not null)
        {
            ModelDropZone.BorderBrush = _dropZoneBorder;
            _dropZoneBorder = null;
        }
    }

    private async void OnModelDrop(object? sender, DragEventArgs e)
    {
        e.Handled = true;
        ResetDropTarget();
        if (DataContext is not MainViewModel viewModel)
        {
            return;
        }

        string? path = e.DataTransfer.TryGetFiles()?
            .Select(item => item.TryGetLocalPath())
            .FirstOrDefault(local => !string.IsNullOrWhiteSpace(local));
        if (string.IsNullOrWhiteSpace(path))
        {
            viewModel.Status = "Letupan tidak berisi file yang dapat dibaca.";
            return;
        }
        if (!path.EndsWith(".gguf", StringComparison.OrdinalIgnoreCase))
        {
            viewModel.Status = "Hanya file .gguf yang dapat diimpor.";
            return;
        }

        await viewModel.ImportDroppedModelAsync(path);
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

    private async void OnDeleteModelClick(object? sender, RoutedEventArgs e)
    {        if (DataContext is not MainViewModel viewModel || viewModel.SelectedModel is not { Installed: true } model)
        {
            return;
        }
        if (TopLevel.GetTopLevel(this) is not Window owner)
        {
            return;
        }

        bool confirmed = await ConfirmDialog.ShowAsync(
            owner,
            "Hapus model?",
            $"File \"{model.Name}\" akan dihapus dari folder model. " +
            "Analisis tidak dapat dijalankan sampai ada model lain yang dimuat.",
            "Hapus model");
        if (confirmed)
        {
            await viewModel.DeleteModelCommand.ExecuteAsync(model);
        }
    }
}
