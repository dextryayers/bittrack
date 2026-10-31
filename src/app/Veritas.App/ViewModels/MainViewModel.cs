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
    private int _selectedTabIndex;

    public ObservableCollection<Claim> ResultClaims { get; } = new();
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
        ResultClaims.Clear();
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
            HasResult = true;
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
        ResultClaims.Clear();
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

    public void Dispose()
    {
        _engine.Dispose();
        _historyService.Dispose();
    }
}
