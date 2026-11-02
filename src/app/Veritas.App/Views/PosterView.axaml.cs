using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using Veritas.App.ViewModels;

namespace Veritas.App.Views;

public partial class PosterView : UserControl
{
    private static readonly string[] ImageExtensions =
    {
        ".png", ".jpg", ".jpeg", ".bmp", ".webp", ".tif", ".tiff",
    };

    private IBrush? _dropZoneBorder;

    public PosterView()
    {
        InitializeComponent();
        DragDrop.SetAllowDrop(PosterDropZone, true);
        PosterDropZone.AddHandler(DragDrop.DragEnterEvent, OnPosterDragEnter);
        PosterDropZone.AddHandler(DragDrop.DragLeaveEvent, OnPosterDragLeave);
        PosterDropZone.AddHandler(DragDrop.DragOverEvent, OnPosterDragOver);
        PosterDropZone.AddHandler(DragDrop.DropEvent, OnPosterDrop);
    }

    private void OnPosterDragEnter(object? sender, DragEventArgs e) => MarkDropTarget(e);

    private void OnPosterDragOver(object? sender, DragEventArgs e) => MarkDropTarget(e);

    private void OnPosterDragLeave(object? sender, DragEventArgs e) => ResetDropTarget();

    private void MarkDropTarget(DragEventArgs e)
    {
        bool hasFile = e.DataTransfer.Contains(DataFormat.File);
        e.DragEffects = hasFile ? DragDropEffects.Copy : DragDropEffects.None;
        e.Handled = true;
        if (hasFile && _dropZoneBorder is null)
        {
            _dropZoneBorder = PosterDropZone.BorderBrush;
            PosterDropZone.BorderBrush = (IBrush?)Application.Current?
                .FindResource("AccentBrush");
        }
    }

    private void ResetDropTarget()
    {
        if (_dropZoneBorder is not null)
        {
            PosterDropZone.BorderBrush = _dropZoneBorder;
            _dropZoneBorder = null;
        }
    }

    private async void OnPosterDrop(object? sender, DragEventArgs e)
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
            viewModel.PosterStatus = "Letupan tidak berisi file yang dapat dibaca.";
            return;
        }
        if (!ImageExtensions.Any(ext => path.EndsWith(ext, StringComparison.OrdinalIgnoreCase)))
        {
            viewModel.PosterStatus = "Format gambar tidak didukung.";
            return;
        }

        await viewModel.LoadPosterAsync(path);
    }
}
