using System;
using System.Collections.ObjectModel;
using System.IO;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Veritas.Core.Config;
using Veritas.Core.Engine;
using Veritas.Core.Models;
using Veritas.Core.Services;

namespace Veritas.App.ViewModels;

public partial class MainViewModel : ObservableObject, IDisposable
{
    private readonly NativeEngine _engine;
    private readonly HistoryService _historyService;
    private readonly SettingsService _settingsService;

    [ObservableProperty]
    private string _inputText = string.Empty;

    [ObservableProperty]
    private string _status = "Siap.";

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private bool _hasResult;

    [ObservableProperty]
    private string _resultVerdict = string.Empty;

    [ObservableProperty]
    private double _resultConfidence;

    [ObservableProperty]
    private string _resultSummary = string.Empty;

    [ObservableProperty]
    private double _resultOpacity;

    [ObservableProperty]
    private int _selectedPageIndex;

    public ObservableCollection<Claim> ResultClaims { get; } = new();
    public ObservableCollection<Evidence> ResultEvidence { get; } = new();
    public ObservableCollection<string> ResultSignals { get; } = new();
    public ObservableCollection<HistoryEntry> History { get; } = new();

    public AppSettings Settings { get; }

    public MainViewModel()
    {
        var baseDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Veritas");
        Directory.CreateDirectory(baseDir);
        _engine = new NativeEngine();
        _historyService = new HistoryService(Path.Combine(baseDir, "history.sqlite"));
        _settingsService = new SettingsService(Path.Combine(baseDir, "settings.json"));
        Settings = _settingsService.Load();
        LoadHistory();
    }

    [RelayCommand]
    private async Task AnalyzeAsync()
    {
        if (string.IsNullOrWhiteSpace(InputText))
        {
            Status = "Masukkan teks terlebih dahulu.";
            return;
        }

        IsBusy = true;
        HasResult = false;
        ResultOpacity = 0;
        ResultClaims.Clear();
        ResultEvidence.Clear();
        ResultSignals.Clear();
        Status = "Menganalisis...";

        try
        {
            var req = new AnalyzeRequest(Text: InputText, LangHint: Settings.Language, Options: new AnalyzeOptions(Threshold: Settings.Threshold));
            var resp = await Task.Run(() => _engine.Analyze(req, ev => Status = $"{ev.Stage}: {ev.Message}"));
            ResultVerdict = resp.Verdict;
            ResultConfidence = resp.Confidence;
            ResultSummary = resp.Summary;
            foreach (var c in resp.Claims)
            {
                ResultClaims.Add(c);
            }
            foreach (var e in resp.Evidence)
            {
                ResultEvidence.Add(e);
            }
            foreach (var s in resp.Heuristics.Signals)
            {
                ResultSignals.Add(s);
            }
            HasResult = true;
            ResultOpacity = 1;
            Status = "Selesai.";
            _historyService.Add(InputText, resp.Verdict, resp.Confidence, resp.Summary);
            LoadHistory();
        }
        catch (Exception ex)
        {
            Status = $"Error: {ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private void Clear()
    {
        InputText = string.Empty;
        HasResult = false;
        ResultOpacity = 0;
        ResultClaims.Clear();
        ResultEvidence.Clear();
        ResultSignals.Clear();
        Status = "Siap.";
    }

    [RelayCommand]
    private void LoadHistory()
    {
        History.Clear();
        foreach (var h in _historyService.List())
        {
            History.Add(h);
        }
    }

    [RelayCommand]
    private void ClearHistory()
    {
        _historyService.Clear();
        LoadHistory();
    }

    [RelayCommand]
    private void SaveSettings()
    {
        _settingsService.Save(Settings);
        Status = "Pengaturan disimpan.";
    }

    [RelayCommand]
    private void ToggleTheme()
    {
        var app = Avalonia.Application.Current;
        if (app is null)
        {
            return;
        }
        var next = app.RequestedThemeVariant == Avalonia.Styling.ThemeVariant.Dark
            ? Avalonia.Styling.ThemeVariant.Light
            : Avalonia.Styling.ThemeVariant.Dark;
        app.RequestedThemeVariant = next;
        Settings.Theme = next == Avalonia.Styling.ThemeVariant.Dark ? "dark" : "light";
        _settingsService.Save(Settings);
    }

    public void Dispose()
    {
        _engine.Dispose();
        _historyService.Dispose();
    }
}
