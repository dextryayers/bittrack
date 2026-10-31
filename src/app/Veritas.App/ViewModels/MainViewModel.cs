using System;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media.Imaging;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Veritas.App.Services;
using Veritas.Core.Config;
using Veritas.Core.Engine;
using Veritas.Core.Models;
using Veritas.Core.Services;

namespace Veritas.App.ViewModels;

public partial class MainViewModel : ObservableObject, IDisposable
{
    private static readonly string[] KnownModelIds =
    {
        "qwen2.5-3b-instruct-q4_k_m",
        "qwen2.5-7b-instruct-q4_k_m",
        "qwen2.5-1.5b-instruct-q4_k_m",
        "qwen2.5-0.5b-instruct-q4_k_m",
    };

    private readonly NativeEngine _engine;
    private readonly HistoryService _historyService;
    private readonly SettingsService _settingsService;
    private Bitmap? _posterPreview;

    [ObservableProperty]
    private string _inputText = string.Empty;

    [ObservableProperty]
    private string _inputUrl = string.Empty;

    [ObservableProperty]
    private string _inputComments = string.Empty;

    [ObservableProperty]
    private string _status = "Siap.";

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private double _analysisProgress;

    [ObservableProperty]
    private bool _hasResult;

    [ObservableProperty]
    private string _resultVerdict = string.Empty;

    [ObservableProperty]
    private double _resultConfidence;

    [ObservableProperty]
    private int _resultTruthPercentage;

    [ObservableProperty]
    private string _resultSummary = string.Empty;

    [ObservableProperty]
    private double _resultOpacity;

    [ObservableProperty]
    private int _selectedPageIndex;

    [ObservableProperty]
    private string _posterImagePath = string.Empty;

    [ObservableProperty]
    private string _posterText = string.Empty;

    [ObservableProperty]
    private string _posterStatus = "Pilih gambar poster untuk mengekstrak teks.";

    [ObservableProperty]
    private bool _isOcrBusy;

    [ObservableProperty]
    private ModelInfo? _selectedModel;

    [ObservableProperty]
    private bool _isDownloading;

    [ObservableProperty]
    private double _downloadPercent;

    [ObservableProperty]
    private bool _hasModels;

    [ObservableProperty]
    private bool _hasPosterPreview;

    [ObservableProperty]
    private bool _hasClaims;

    [ObservableProperty]
    private bool _hasEvidence;

    [ObservableProperty]
    private bool _hasSignals;

    [ObservableProperty]
    private bool _hasWarnings;

    [ObservableProperty]
    private bool _hasRedFlags;

    [ObservableProperty]
    private string _resultModel = string.Empty;

    public ObservableCollection<Claim> ResultClaims { get; } = new();
    public ObservableCollection<Evidence> ResultEvidence { get; } = new();
    public ObservableCollection<string> ResultSignals { get; } = new();
    public ObservableCollection<string> ResultWarnings { get; } = new();
    public ObservableCollection<string> ResultRedFlags { get; } = new();
    public ObservableCollection<HistoryEntry> History { get; } = new();
    public ObservableCollection<ModelInfo> Models { get; } = new();

    public string[] LanguageOptions { get; } = { "id", "en" };
    public string[] ThemeOptions { get; } = { "system", "light", "dark" };

    [ObservableProperty]
    private bool _hasHistory;

    public AppSettings Settings { get; }

    public Bitmap? PosterPreview
    {
        get => _posterPreview;
        private set
        {
            var previous = _posterPreview;
            if (SetProperty(ref _posterPreview, value))
            {
                previous?.Dispose();
                HasPosterPreview = value is not null;
            }
        }
    }

    public MainViewModel()
    {
        var baseDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Veritas");
        Directory.CreateDirectory(baseDir);
        _settingsService = new SettingsService(Path.Combine(baseDir, "settings.json"));
        Settings = _settingsService.Load();
        SanitizeSettings();

        var config = JsonSerializer.Serialize(new
        {
            model_id = Settings.ModelId,
            threads = Math.Clamp(Environment.ProcessorCount, 1, 8),
            context_tokens = 8192,
            max_output_tokens = 1400,
            online_evidence = Settings.OnlineEvidence,
        });
        _engine = new NativeEngine(config);
        _historyService = new HistoryService(Path.Combine(baseDir, "history.sqlite"));
        ApplyTheme(Settings.Theme);
        LoadHistory();
        _ = RefreshModelsAsync();
    }

    private void SanitizeSettings()
    {
        if (!KnownModelIds.Contains(Settings.ModelId))
        {
            Settings.ModelId = KnownModelIds[0];
        }
        if (Settings.Language is not ("id" or "en"))
        {
            Settings.Language = LanguageOptions[0];
        }
        if (Settings.Theme is not ("system" or "light" or "dark"))
        {
            Settings.Theme = ThemeOptions[0];
        }
        Settings.Threshold = Math.Clamp(Settings.Threshold, 0.0, 1.0);
        _settingsService.Save(Settings);
    }

    [RelayCommand]
    private async Task AnalyzeAsync()
    {
        bool hasText = !string.IsNullOrWhiteSpace(InputText);
        bool hasUrl = !string.IsNullOrWhiteSpace(InputUrl);
        bool hasComments = !string.IsNullOrWhiteSpace(InputComments);
        if (!hasText && !hasUrl && !hasComments)
        {
            Status = "Masukkan teks berita, tautan, atau komentar terlebih dahulu.";
            return;
        }

        var request = new AnalyzeRequest(
            Text: NullIfBlank(InputText),
            Url: NullIfBlank(InputUrl),
            Comments: NullIfBlank(InputComments),
            LangHint: Settings.Language,
            Options: BuildOptions());

        string historyInput = hasUrl
            ? InputUrl.Trim()
            : hasText
                ? InputText
                : "Komentar: " + InputComments.Trim();

        await RunAnalysisAsync(request, historyInput);
    }

    [RelayCommand]
    private async Task AnalyzePosterAsync()
    {
        if (string.IsNullOrWhiteSpace(PosterText))
        {
            PosterStatus = "Teks poster masih kosong. Pilih gambar atau tempel teks hasil OCR.";
            return;
        }

        var request = new AnalyzeRequest(
            PosterText: PosterText,
            LangHint: Settings.Language,
            Options: BuildOptions());

        string historyInput = string.IsNullOrWhiteSpace(PosterImagePath)
            ? "Poster: " + PosterText[..Math.Min(80, PosterText.Length)]
            : "Poster: " + Path.GetFileName(PosterImagePath);

        await RunAnalysisAsync(request, historyInput);
    }

    private AnalyzeOptions BuildOptions() => new(
        Threshold: Settings.Threshold,
        OnlineEvidence: Settings.OnlineEvidence);

    private async Task RunAnalysisAsync(AnalyzeRequest request, string historyInput)
    {
        IsBusy = true;
        HasResult = false;
        ResultOpacity = 0;
        AnalysisProgress = 0;
        ResultClaims.Clear();
        ResultEvidence.Clear();
        ResultSignals.Clear();
        ResultWarnings.Clear();
        ResultRedFlags.Clear();
        Status = "Menganalisis...";

        try
        {
            var resp = await Task.Run(() => _engine.Analyze(request, ReportProgress));
            ApplyResult(resp);
            _historyService.Add(historyInput, resp.Verdict, resp.Confidence, resp.Summary);
            LoadHistory();
            Status = $"Selesai dalam mode lokal: {resp.Audit.Model}";
        }
        catch (Exception ex)
        {
            Status = $"Gagal: {ex.Message}";
        }
        finally
        {
            IsBusy = false;
            AnalysisProgress = 0;
        }
    }

    private void ReportProgress(ProgressEvent ev)
    {
        void Update()
        {
            AnalysisProgress = ev.Percent * 100.0;
            Status = ev.Stage switch
            {
                "fetch" => "Mengambil isi tautan...",
                "retrieve" => "Mencari bukti dari sumber tepercaya...",
                "load_model" => ev.Message,
                "generate" => $"Model menganalisis: {ev.Message}",
                "map" => "Menyusun hasil...",
                "done" => "Selesai.",
                _ => $"{ev.Stage}: {ev.Message}",
            };
        }

        if (Application.Current is null)
        {
            Update();
            return;
        }
        Dispatcher.UIThread.Post(Update);
    }

    private void ApplyResult(AnalyzeResponse resp)
    {
        ResultVerdict = resp.Verdict;
        ResultConfidence = resp.Confidence;
        ResultTruthPercentage = resp.TruthPercentage;
        ResultSummary = resp.Summary;
        foreach (var claim in resp.Claims)
        {
            ResultClaims.Add(claim);
        }
        foreach (var evidence in resp.Evidence)
        {
            ResultEvidence.Add(evidence);
        }
        foreach (var signal in resp.Heuristics.Signals)
        {
            ResultSignals.Add(signal);
        }
        foreach (var warning in resp.Warnings)
        {
            ResultWarnings.Add(warning);
        }
        foreach (var flag in resp.RedFlags)
        {
            ResultRedFlags.Add(flag);
        }
        HasClaims = ResultClaims.Count > 0;
        HasEvidence = ResultEvidence.Count > 0;
        HasSignals = ResultSignals.Count > 0;
        HasWarnings = ResultWarnings.Count > 0;
        HasRedFlags = ResultRedFlags.Count > 0;
        ResultModel = resp.Audit.Model;
        HasResult = true;
        ResultOpacity = 1;
        SelectedPageIndex = 1;
    }

    [RelayCommand]
    private async Task PickPosterAsync()
    {
        string? path = await PickImageAsync();
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        PosterImagePath = path;
        try
        {
            PosterPreview = new Bitmap(path);
        }
        catch (Exception)
        {
            PosterPreview = null;
        }

        IsOcrBusy = true;
        PosterStatus = "Mengekstrak teks dari gambar...";
        try
        {
            string text = await PosterOcrService.ExtractTextAsync(path);
            PosterText = text;
            PosterStatus = string.IsNullOrWhiteSpace(text)
                ? "Tidak ada teks yang terbaca. Coba gambar yang lebih jelas atau tempel teks manual."
                : $"Teks terbaca: {text.Length} karakter. Periksa sebelum dianalisis.";
        }
        catch (Exception ex)
        {
            PosterStatus = $"OCR gagal: {ex.Message}";
        }
        finally
        {
            IsOcrBusy = false;
        }
    }

    [RelayCommand]
    private void ClearPoster()
    {
        PosterImagePath = string.Empty;
        PosterText = string.Empty;
        PosterStatus = "Pilih gambar poster untuk mengekstrak teks.";
        PosterPreview = null;
    }

    private static async Task<string?> PickImageAsync()
    {
        if (Application.Current?.ApplicationLifetime
            is not Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop
            || desktop.MainWindow is null)
        {
            return null;
        }

        var files = await desktop.MainWindow.StorageProvider.OpenFilePickerAsync(
            new FilePickerOpenOptions
            {
                Title = "Pilih poster berita",
                AllowMultiple = false,
                FileTypeFilter = new[]
                {
                    new FilePickerFileType("Gambar")
                    {
                        Patterns = new[] { "*.png", "*.jpg", "*.jpeg", "*.bmp", "*.webp", "*.tif", "*.tiff" },
                    },
                },
            });
        return files.Count > 0 ? files[0].TryGetLocalPath() : null;
    }

    [RelayCommand]
    private void Clear()
    {
        InputText = string.Empty;
        InputUrl = string.Empty;
        InputComments = string.Empty;
        HasResult = false;
        ResultOpacity = 0;
        ResultClaims.Clear();
        ResultEvidence.Clear();
        ResultSignals.Clear();
        ResultWarnings.Clear();
        ResultRedFlags.Clear();
        HasClaims = false;
        HasEvidence = false;
        HasSignals = false;
        HasWarnings = false;
        HasRedFlags = false;
        Status = "Siap.";
    }

    [RelayCommand]
    private void LoadHistory()
    {
        History.Clear();
        foreach (var item in _historyService.List())
        {
            History.Add(item);
        }
        HasHistory = History.Count > 0;
    }

    [RelayCommand]
    private void ClearHistory()
    {
        _historyService.Clear();
        LoadHistory();
    }

    [RelayCommand]
    private async Task RefreshModelsAsync()
    {
        try
        {
            var list = await Task.Run(() => _engine.ListModels());
            string previousId = SelectedModel?.Id ?? Settings.ModelId;
            Models.Clear();
            foreach (var model in list)
            {
                Models.Add(model);
            }
            HasModels = Models.Count > 0;
            SelectedModel = Models.FirstOrDefault(m => m.Id == previousId)
                            ?? Models.FirstOrDefault(m => m.Installed)
                            ?? Models.FirstOrDefault();
        }
        catch (Exception ex)
        {
            Status = $"Daftar model gagal dimuat: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task DownloadModelAsync()
    {
        if (SelectedModel is null || IsDownloading)
        {
            return;
        }

        var model = SelectedModel;
        IsDownloading = true;
        DownloadPercent = 0;
        Status = $"Mengunduh {model.Name}...";
        try
        {
            bool completed = await Task.Run(() => _engine.DownloadModel(model.Id, ev =>
            {
                Dispatcher.UIThread.Post(() =>
                {
                    DownloadPercent = ev.Percent * 100.0;
                    if (ev.Stage == "download_error")
                    {
                        Status = $"Unduhan gagal: {ev.Message}";
                    }
                    else if (ev.Stage == "download_done")
                    {
                        Status = "Model terunduh dan SHA-256 terverifikasi.";
                    }
                    else if (ev.Stage == "download")
                    {
                        Status = $"Mengunduh {model.Id}: {ev.Message}";
                    }
                });
            }));
            Status = completed
                ? "Model siap digunakan tanpa koneksi."
                : "Unduhan dibatalkan.";
        }
        catch (Exception ex)
        {
            Status = $"Unduh gagal: {ex.Message}";
        }
        finally
        {
            IsDownloading = false;
            DownloadPercent = 0;
            await RefreshModelsAsync();
        }
    }

    [RelayCommand]
    private void SaveSettings()
    {
        if (SelectedModel is not null)
        {
            Settings.ModelId = SelectedModel.Id;
        }
        _settingsService.Save(Settings);
        ApplyTheme(Settings.Theme);
        Status = "Pengaturan disimpan. Perubahan model dan bukti daring berlaku setelah aplikasi dibuka ulang.";
    }

    [RelayCommand]
    private void ToggleTheme()
    {
        var app = Application.Current;
        if (app is null)
        {
            return;
        }
        var next = app.ActualThemeVariant == Avalonia.Styling.ThemeVariant.Dark
            ? Avalonia.Styling.ThemeVariant.Light
            : Avalonia.Styling.ThemeVariant.Dark;
        app.RequestedThemeVariant = next;
        Settings.Theme = next == Avalonia.Styling.ThemeVariant.Dark ? "dark" : "light";
        _settingsService.Save(Settings);
        Status = "Tema diperbarui.";
    }

    private static string? NullIfBlank(string value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static void ApplyTheme(string theme)
    {
        var app = Application.Current;
        if (app is null)
        {
            return;
        }
        app.RequestedThemeVariant = theme switch
        {
            "dark" => Avalonia.Styling.ThemeVariant.Dark,
            "light" => Avalonia.Styling.ThemeVariant.Light,
            _ => Avalonia.Styling.ThemeVariant.Default,
        };
    }

    public void Dispose()
    {
        _engine.Dispose();
        _historyService.Dispose();
        PosterPreview = null;
    }
}
