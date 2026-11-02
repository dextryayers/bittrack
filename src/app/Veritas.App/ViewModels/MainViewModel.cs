using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Input.Platform;
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
    private readonly SemaphoreSlim _modelGate = new(1, 1);
    private NativeEngine _engine;
    private readonly HistoryService _historyService;
    private readonly SettingsService _settingsService;
    private readonly string _modelDirectory;
    private Bitmap? _posterPreview;
    private AnalyzeResponse? _lastResponse;
    private int _engineGeneration;
    private bool _suppressModelReload;

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
    private string _resultVerdictLabel = string.Empty;

    [ObservableProperty]
    private double _resultConfidence;

    [ObservableProperty]
    private string _resultConfidenceDisplay = string.Empty;

    [ObservableProperty]
    private int _resultTruthPercentage;

    [ObservableProperty]
    private int _resultAiPercentage;

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
    private bool _isModelLoading;

    [ObservableProperty]
    private string _modelLoadStatus = "Belum ada model yang dimuat.";

    [ObservableProperty]
    private string _selectedModelMeta = "Unggah model GGUF untuk memulai.";

    [ObservableProperty]
    private bool _hasUsableModel;

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

    [ObservableProperty]
    private string _resultAuditLine = string.Empty;

    [ObservableProperty]
    private int _resultDomainScore;

    [ObservableProperty]
    private int _resultBuzzerScore;

    [ObservableProperty]
    private int _resultClickbaitScore;

    [ObservableProperty]
    private string _historyQuery = string.Empty;

    [ObservableProperty]
    private bool _hasHistory;

    [ObservableProperty]
    private bool _hasRecentHistory;

    [ObservableProperty]
    private bool _isImportingModel;

    [ObservableProperty]
    private string _importStatus = string.Empty;

    public ObservableCollection<Claim> ResultClaims { get; } = new();    public ObservableCollection<Evidence> ResultEvidence { get; } = new();
    public ObservableCollection<string> ResultSignals { get; } = new();
    public ObservableCollection<string> ResultWarnings { get; } = new();
    [ObservableProperty]
    private bool _hasActivity;

    public ObservableCollection<string> ResultRedFlags { get; } = new();
    public ObservableCollection<string> ActivityLog { get; } = new();
    public ObservableCollection<HistoryEntry> History { get; } = new();
    public ObservableCollection<HistoryEntry> RecentHistory { get; } = new();
    public ObservableCollection<ModelInfo> Models { get; } = new();

    public string[] LanguageOptions { get; } = { "id", "en" };
    public string[] ThemeOptions { get; } = { "system", "light", "dark" };

    public AppSettings Settings { get; }

    public string ModelDirectory => _modelDirectory;

    public string AppVersion => "1.0.0-alpha";

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
        _modelDirectory = Path.Combine(baseDir, "models");
        Directory.CreateDirectory(_modelDirectory);

        _settingsService = new SettingsService(Path.Combine(baseDir, "settings.json"));
        Settings = _settingsService.Load();
        SanitizeSettings();
        Settings.PropertyChanged += (_, _) => _settingsService.Save(Settings);

        _engine = CreateEngine();
        _historyService = new HistoryService(Path.Combine(baseDir, "history.sqlite"));
        ApplyTheme(Settings.Theme);
        LoadHistory();
        _ = InitializeModelsAsync();
    }

    private NativeEngine CreateEngine() => new(JsonSerializer.Serialize(new
    {
        model_id = string.IsNullOrWhiteSpace(Settings.ModelId) ? null : Settings.ModelId,
        threads = Math.Clamp(Environment.ProcessorCount, 1, 8),
        context_tokens = 8192,
        max_output_tokens = 1400,
        online_evidence = Settings.OnlineEvidence,
    }));

    private void SanitizeSettings()
    {
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

    private async Task InitializeModelsAsync()
    {
        await RefreshModelsAsync();
        if (SelectedModel is { Installed: true, Compatible: true } model)
        {
            await SwitchModelAsync(model);
        }
    }

    [RelayCommand]
    private async Task AnalyzeAsync()
    {
        bool hasText = !string.IsNullOrWhiteSpace(InputText);
        bool hasUrl = !string.IsNullOrWhiteSpace(InputUrl);
        bool hasComments = !string.IsNullOrWhiteSpace(InputComments);
        if (!hasText && !hasUrl && !hasComments)
        {
            Status = "Masukkan teks, tautan, atau komentar terlebih dahulu.";
            return;
        }

        if (IsBusy)
        {
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
                ? InputText.Trim()
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
        if (IsBusy)
        {
            return;
        }

        string posterInput = PosterText.Trim();
        if (!string.IsNullOrWhiteSpace(PosterImagePath) && File.Exists(PosterImagePath))
        {
            try
            {
                int width = 0;
                int height = 0;
                long bytes = new FileInfo(PosterImagePath).Length;
                if (PosterPreview is not null)
                {
                    width = PosterPreview.PixelSize.Width;
                    height = PosterPreview.PixelSize.Height;
                }
                else
                {
                    using var bitmap = new Avalonia.Media.Imaging.Bitmap(PosterImagePath);
                    width = bitmap.PixelSize.Width;
                    height = bitmap.PixelSize.Height;
                }
                if (width > 0 && height > 0)
                {
                    posterInput = $"[Gambar: {Path.GetFileName(PosterImagePath)}, {width}x{height}px, {bytes / 1024} KB]\n{posterInput}";
                }
            }
            catch (Exception)
            {
                // Metadata is optional context; OCR text remains the primary input.
            }
        }

        var request = new AnalyzeRequest(
            PosterText: posterInput,
            LangHint: Settings.Language,
            Options: BuildOptions());

        string historyInput = string.IsNullOrWhiteSpace(PosterImagePath)
            ? "Poster: " + PosterText[..Math.Min(80, PosterText.Length)]
            : "Poster: " + Path.GetFileName(PosterImagePath);

        await RunAnalysisAsync(request, historyInput);
    }

    private AnalyzeOptions BuildOptions() => new(
        Threshold: Settings.Threshold,
        IncludeHeuristics: true,
        MaxClaims: 8,
        OnlineEvidence: Settings.OnlineEvidence,
        StrictVerification: Settings.StrictVerification,
        DeepVerification: Settings.DeepVerification);

    private async Task RunAnalysisAsync(AnalyzeRequest request, string historyInput)
    {
        IsBusy = true;
        HasResult = false;
        ResultOpacity = 0;
        AnalysisProgress = 0;
        ClearResultCollections();
        ActivityLog.Clear();
        HasActivity = false;
        Status = "Menganalisis...";

        try
        {
            AnalyzeResponse resp;
            // Serialize native calls: a model switch disposes and replaces the
            // engine handle, and using the old pointer mid-inference would crash
            // the process rather than return an error.
            await _modelGate.WaitAsync();
            try
            {
                resp = await Task.Run(() => _engine.Analyze(request, ReportProgress));
            }
            finally
            {
                _modelGate.Release();
            }
            ApplyResult(resp, navigate: true);
            _historyService.Add(
                historyInput,
                resp.Verdict,
                resp.Confidence,
                resp.Summary,
                JsonSerializer.Serialize(resp));
            LoadHistory();
            Status = "Analisis selesai. Periksa bukti sebelum menyimpulkan.";
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
            string line = $"{DateTime.Now:HH:mm:ss}  {ev.Stage.ToUpperInvariant()}  {ev.Message}";
            if (!ActivityLog.Contains(line))
            {
                ActivityLog.Add(line);
                HasActivity = true;
            }
            Status = ev.Stage switch
            {
                "fetch" => "Mengambil isi tautan...",
                "retrieve" => "Mencari bukti dari sumber tepercaya...",
                "load_model" => ev.Message,
                "prompt" => "Menyusun prompt...",
                "generate" => $"Model menganalisis: {ev.Message}",
                "retry" => "Mengulang karena keluaran tidak sesuai skema...",
                "verify" => "Verifikasi ketat: menjalankan inferensi kedua...",
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

    private void ApplyResult(AnalyzeResponse resp, bool navigate)
    {
        _lastResponse = resp;
        ResultVerdict = resp.Verdict;
        ResultVerdictLabel = VerdictLabel(resp.Verdict);
        ResultConfidence = resp.Confidence;
        ResultConfidenceDisplay = $"{resp.Confidence * 100:0}%";
        ResultTruthPercentage = resp.TruthPercentage;
        ResultAiPercentage = resp.AiPercentage;
        ResultSummary = resp.Summary;
        ResultDomainScore = (int)Math.Round(resp.Heuristics.DomainScore * 100);
        ResultBuzzerScore = (int)Math.Round(resp.Heuristics.BuzzerScore * 100);
        ResultClickbaitScore = (int)Math.Round(resp.Heuristics.ClickbaitScore * 100);

        ClearResultCollections();
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
        ResultAuditLine = $"{resp.Audit.Model} · prompt {resp.Audit.PromptVersion} · engine {resp.Audit.EngineVersion}";
        HasResult = true;
        ResultOpacity = 1;
        if (navigate)
        {
            SelectedPageIndex = 1;
        }
    }

    private void ClearResultCollections()
    {
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
    }

    private static string VerdictLabel(string verdict) => verdict switch
    {
        "hoax" => "Hoaks",
        "misleading" => "Menyesatkan",
        "clickbait" => "Clickbait",
        "buzzer" => "Buzzer",
        "valid" => "Valid",
        "unverifiable" => "Tidak terverifikasi",
        _ => verdict,
    };

    [RelayCommand]
    private async Task PickPosterAsync()
    {
        string? path = await PickImageAsync();
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }
        await LoadPosterAsync(path);
    }

    public async Task LoadPosterAsync(string path)
    {
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
            is not IClassicDesktopStyleApplicationLifetime desktop
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
        ClearResult();
        Status = "Siap.";
    }

    private void ClearResult()
    {
        HasResult = false;
        ResultOpacity = 0;
        _lastResponse = null;
        ClearResultCollections();
    }

    [RelayCommand]
    private void OpenAiDetect()
    {
        SelectedPageIndex = 6;
    }

    [RelayCommand]
    private void OpenAnalyze()
    {
        SelectedPageIndex = 1;
    }

    [RelayCommand]
    private void OpenPoster()
    {
        SelectedPageIndex = 2;
    }

    [RelayCommand]
    private void OpenSettings()
    {
        SelectedPageIndex = 4;
    }

    [RelayCommand]
    private void Navigate(string? page)
    {
        if (int.TryParse(page, out int index))
        {
            SelectedPageIndex = Math.Clamp(index, 0, 5);
        }
    }

    [RelayCommand]
    private async Task ExportReportAsync()
    {
        if (_lastResponse is null)
        {
            Status = "Belum ada hasil untuk diekspor.";
            return;
        }
        if (Application.Current?.ApplicationLifetime
            is not IClassicDesktopStyleApplicationLifetime { MainWindow: { } window })
        {
            return;
        }

        var file = await window.StorageProvider.SaveFilePickerAsync(new FilePickerSaveOptions
        {
            Title = "Simpan laporan Veritas",
            SuggestedFileName = $"veritas-{DateTime.Now:yyyyMMdd-HHmmss}.md",
            FileTypeChoices = new[]
            {
                new FilePickerFileType("Markdown") { Patterns = new[] { "*.md" } },
                new FilePickerFileType("Teks") { Patterns = new[] { "*.txt" } },
            },
            DefaultExtension = "md",
        });
        if (file is null)
        {
            return;
        }

        string? path = file.TryGetLocalPath();
        if (string.IsNullOrWhiteSpace(path))
        {
            Status = "Lokasi penyimpanan tidak dapat diakses.";
            return;
        }

        try
        {
            await File.WriteAllTextAsync(path, BuildReportMarkdown(_lastResponse));
            Status = $"Laporan disimpan: {path}";
            try
            {
                Process.Start(new ProcessStartInfo { FileName = Path.GetDirectoryName(path)!, UseShellExecute = true });
            }
            catch
            {
                // Best effort: the file is still saved.
            }
        }
        catch (Exception ex)
        {
            Status = $"Gagal menyimpan laporan: {ex.Message}";
        }
    }

    private static string BuildReportMarkdown(AnalyzeResponse resp)
    {
        var sb = new StringBuilder();
        sb.AppendLine("# Veritas — Laporan Analisis");
        sb.AppendLine();
        sb.AppendLine($"- **Verdict model:** {VerdictLabel(resp.Verdict)}");
        sb.AppendLine($"- **Keyakinan model:** {resp.Confidence * 100:0}%");
        sb.AppendLine($"- **Perkiraan kebenaran (belum terkalibrasi):** {resp.TruthPercentage}%");
        sb.AppendLine($"- **Perkiraan tulisan AI (eksperimental):** {resp.AiPercentage}%");
        sb.AppendLine($"- **Model:** {resp.Audit.Model}");
        sb.AppendLine($"- **Prompt:** {resp.Audit.PromptVersion}");
        sb.AppendLine($"- **Input hash:** {resp.Audit.InputHash}");
        sb.AppendLine();
        sb.AppendLine("## Ringkasan");
        sb.AppendLine();
        sb.AppendLine(resp.Summary);
        sb.AppendLine();
        if (resp.Claims.Length > 0)
        {
            sb.AppendLine("## Klaim");
            sb.AppendLine();
            foreach (var claim in resp.Claims)
            {
                sb.AppendLine($"### {claim.Id}. {claim.Text}");
                sb.AppendLine($"- Verdict: {claim.VerdictLabel}");
                sb.AppendLine($"- Jenis: {claim.TypeLabel}");
                sb.AppendLine($"- Keyakinan: {claim.ConfidenceDisplay}");
                sb.AppendLine($"- Rujukan bukti: {claim.EvidenceIdsDisplay}");
                if (claim.HasReasoning)
                {
                    sb.AppendLine($"- Alasan: {claim.Reasoning}");
                }
                sb.AppendLine();
            }
        }
        if (resp.Evidence.Length > 0)
        {
            sb.AppendLine("## Bukti Referensi");
            sb.AppendLine();
            foreach (var evidence in resp.Evidence)
            {
                sb.AppendLine($"### [{evidence.Id}] {evidence.Source}");
                sb.AppendLine($"- Jenis: {evidence.KindLabel}");
                sb.AppendLine($"- Tanggal: {evidence.DateDisplay}");
                sb.AppendLine($"- URL: {evidence.Url}");
                sb.AppendLine();
                sb.AppendLine(evidence.Text);
                sb.AppendLine();
            }
        }
        if (resp.Heuristics.Signals.Length > 0)
        {
            sb.AppendLine("## Sinyal Heuristik (konteks, bukan verdict)");
            sb.AppendLine();
            foreach (var signal in resp.Heuristics.Signals)
            {
                sb.AppendLine($"- {signal}");
            }
            sb.AppendLine();
        }
        if (resp.Warnings.Length > 0)
        {
            sb.AppendLine("## Catatan Engine");
            sb.AppendLine();
            foreach (var warning in resp.Warnings)
            {
                sb.AppendLine($"- {warning}");
            }
            sb.AppendLine();
        }
        if (resp.RedFlags.Length > 0)
        {
            sb.AppendLine("## Tanda Merah");
            sb.AppendLine();
            foreach (var flag in resp.RedFlags)
            {
                sb.AppendLine($"- {flag}");
            }
            sb.AppendLine();
        }
        sb.AppendLine("---");
        sb.AppendLine("Dibuat oleh Veritas. Hasil adalah bahan pemeriksaan, bukan putusan final.");
        return sb.ToString();
    }
    [RelayCommand]
    private async Task CopyResultAsync()
    {
        if (_lastResponse is null)
        {
            return;
        }
        await SetClipboardTextAsync(BuildResultText(_lastResponse));
        Status = "Hasil disalin ke papan klip.";
    }

    private static string BuildResultText(AnalyzeResponse resp)
    {
        var sb = new StringBuilder();
        sb.AppendLine("VERITAS - HASIL ANALISIS");
        sb.AppendLine(VerdictLabel(resp.Verdict).ToUpperInvariant());
        sb.AppendLine($"Keyakinan model: {resp.Confidence * 100:0}%");
        sb.AppendLine($"Perkiraan kebenaran (belum terkalibrasi): {resp.TruthPercentage}%");
        sb.AppendLine($"Perkiraan tulisan AI (eksperimental): {resp.AiPercentage}%");
        sb.AppendLine();
        sb.AppendLine(resp.Summary);
        if (resp.Claims.Length > 0)
        {
            sb.AppendLine();
            sb.AppendLine("KLAIM");
            foreach (var claim in resp.Claims)
            {
                sb.AppendLine($"{claim.Id}. [{VerdictLabel(claim.Verdict)}] {claim.Text}");
                if (!string.IsNullOrWhiteSpace(claim.Reasoning))
                {
                    sb.AppendLine($"   {claim.Reasoning}");
                }
            }
        }
        if (resp.Evidence.Length > 0)
        {
            sb.AppendLine();
            sb.AppendLine("BUKTI");
            foreach (var evidence in resp.Evidence)
            {
                sb.AppendLine($"[{evidence.Id}] {evidence.Source} — {evidence.Url}");
                sb.AppendLine($"   {evidence.Text}");
            }
        }
        if (resp.Warnings.Length > 0)
        {
            sb.AppendLine();
            sb.AppendLine("CATATAN");
            foreach (var warning in resp.Warnings)
            {
                sb.AppendLine($"- {warning}");
            }
        }
        sb.AppendLine();
        sb.AppendLine($"Model: {resp.Audit.Model}");
        sb.AppendLine($"Prompt: {resp.Audit.PromptVersion}");
        sb.AppendLine($"Input hash: {resp.Audit.InputHash}");
        return sb.ToString();
    }

    [RelayCommand]
    private async Task OpenEvidenceAsync(Evidence? evidence)
    {
        if (evidence is null)
        {
            return;
        }
        if (!Uri.TryCreate(evidence.Url, UriKind.Absolute, out var uri)
            || (uri.Scheme != Uri.UriSchemeHttp && uri.Scheme != Uri.UriSchemeHttps))
        {
            Status = "Tautan bukti tidak valid.";
            return;
        }

        try
        {
            Process.Start(new ProcessStartInfo { FileName = uri.AbsoluteUri, UseShellExecute = true });
        }
        catch (Exception ex)
        {
            Status = $"Gagal membuka tautan: {ex.Message}";
        }
        await Task.CompletedTask;
    }

    [RelayCommand]
    private async Task CopyEvidenceAsync(Evidence? evidence)
    {
        if (evidence is null)
        {
            return;
        }
        await SetClipboardTextAsync(evidence.Url);
        Status = "Tautan bukti disalin.";
    }

    private static async Task SetClipboardTextAsync(string text)
    {
        if (Application.Current?.ApplicationLifetime
            is IClassicDesktopStyleApplicationLifetime { MainWindow: { } window })
        {
            var clipboard = window.Clipboard;
            if (clipboard is not null)
            {
                await clipboard.SetTextAsync(text);
            }
        }
    }

    [RelayCommand]
    private void LoadHistory()
    {
        History.Clear();
        foreach (var item in _historyService.Search(HistoryQuery))
        {
            History.Add(item);
        }
        RecentHistory.Clear();
        foreach (var item in History.Take(3))
        {
            RecentHistory.Add(item);
        }
        HasHistory = History.Count > 0;
        HasRecentHistory = RecentHistory.Count > 0;
    }

    partial void OnHistoryQueryChanged(string value) => LoadHistory();

    [RelayCommand]
    private void OpenHistory(HistoryEntry? entry)
    {
        if (entry is null)
        {
            return;
        }
        if (!entry.CanReopen)
        {
            Status = "Hasil lengkap tidak tersimpan untuk catatan lama ini. Jalankan analisis ulang.";
            return;
        }
        try
        {
            var resp = JsonSerializer.Deserialize<AnalyzeResponse>(entry.ResultJson);
            if (resp is null)
            {
                Status = "Hasil tersimpan tidak dapat dibaca.";
                return;
            }
            ApplyResult(resp, navigate: true);
            Status = $"Membuka riwayat {entry.CreatedAtDisplay}.";
        }
        catch (JsonException ex)
        {
            Status = $"Hasil tersimpan rusak: {ex.Message}";
        }
    }

    [RelayCommand]
    private void DeleteHistory(HistoryEntry? entry)
    {
        if (entry is null)
        {
            return;
        }
        if (_historyService.Delete(entry.Id))
        {
            Status = "Satu catatan riwayat dihapus.";
            LoadHistory();
        }
    }

    [RelayCommand]
    private void ClearHistory()
    {
        _historyService.Clear();
        LoadHistory();
        Status = "Riwayat dihapus.";
    }

    [RelayCommand]
    private async Task RefreshModelsAsync()
    {
        try
        {
            IReadOnlyList<ModelInfo> list;
            await _modelGate.WaitAsync();
            try
            {
                list = await Task.Run(() => _engine.ListModels());
            }
            finally
            {
                _modelGate.Release();
            }
            string previousId = SelectedModel?.Id ?? Settings.ModelId;
            _suppressModelReload = true;
            try
            {
                Models.Clear();
                foreach (var model in list)
                {
                    Models.Add(model);
                }
                HasModels = Models.Count > 0;
                SelectedModel = Models.FirstOrDefault(m => m.Id == previousId)
                                ?? Models.FirstOrDefault(m => m.Installed && m.Compatible)
                                ?? Models.FirstOrDefault(m => m.Installed)
                                ?? Models.FirstOrDefault();
            }
            finally
            {
                _suppressModelReload = false;
            }

            if (SelectedModel is { } selected)
            {
                ApplyModelSummary(selected);
            }
            else
            {
                SelectedModelMeta = "Belum ada file model di folder model.";
                ModelLoadStatus = "Unggah model GGUF untuk memulai.";
            }
        }
        catch (Exception ex)
        {
            Status = $"Daftar model gagal dimuat: {ex.Message}";
        }
    }

    private void ApplyModelSummary(ModelInfo model)
    {
        SelectedModelMeta = $"{model.Name}\n{model.MetaLine}";
        if (!model.Compatible)
        {
            HasUsableModel = false;
            ModelLoadStatus = "Model tidak kompatibel untuk analisis.";
        }
        else if (IsModelLoading)
        {
            ModelLoadStatus = "Memuat model...";
        }
        else if (!HasUsableModel)
        {
            ModelLoadStatus = "Model belum dimuat.";
        }
    }

    [RelayCommand]
    private async Task UploadModelAsync()
    {
        if (IsBusy || IsModelLoading || IsImportingModel)
        {
            return;
        }

        if (Application.Current?.ApplicationLifetime
            is not IClassicDesktopStyleApplicationLifetime { MainWindow: { } window })
        {
            Status = "Jendela utama tidak tersedia.";
            return;
        }

        var files = await window.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Pilih file model GGUF",
            AllowMultiple = false,
            FileTypeFilter = new[]
            {
                new FilePickerFileType("Model GGUF") { Patterns = new[] { "*.gguf", "*.GGUF" } }
            }
        });
        if (files.Count == 0)
        {
            return;
        }

        string? sourcePath = files[0].TryGetLocalPath();
        if (string.IsNullOrWhiteSpace(sourcePath) || !File.Exists(sourcePath))
        {
            Status = "File model tidak dapat dibaca dari penyimpanan lokal.";
            return;
        }

        await ImportModelAsync(sourcePath);
    }

    public async Task ImportDroppedModelAsync(string sourcePath)
    {
        if (IsBusy || IsModelLoading || IsImportingModel)
        {
            Status = "Tunggu proses yang sedang berjalan sebelum mengimpor model.";
            return;
        }
        if (!File.Exists(sourcePath))
        {
            Status = "File yang dijatuhkan tidak ditemukan.";
            return;
        }
        await ImportModelAsync(sourcePath);
    }

    private async Task ImportModelAsync(string sourcePath)
    {        var fileInfo = new FileInfo(sourcePath);
        if (fileInfo.Length < 1024 * 1024)
        {
            Status = "File terlalu kecil untuk menjadi model GGUF yang valid.";
            return;
        }

        ModelInspection inspection;
        try
        {
            inspection = await Task.Run(() => _engine.InspectModel(sourcePath));
        }
        catch (Exception ex)
        {
            Status = $"File bukan model GGUF yang dapat dibaca: {ex.Message}";
            return;
        }

        if (!inspection.Compatible)
        {
            Status = $"Model ditolak: {inspection.Error ?? "format tidak didukung"}.";
            return;
        }

        string destName = SanitizeModelFileName(fileInfo.Name);
        string destPath = UniqueModelPath(destName);
        string tempPath = destPath + ".part";

        IsImportingModel = true;
        ImportStatus = $"Menyiapkan {destName}...";
        try
        {
            await Task.Run(() => CopyFileWithProgress(sourcePath, tempPath, destName));
            File.Move(tempPath, destPath);
            Status = inspection.Warnings.Length > 0
                ? $"Model {destName} diimpor. Catatan: {inspection.Warnings[0]}"
                : $"Model {destName} diimpor dan lolos inspeksi metadata.";
        }
        catch (Exception ex)
        {
            TryDelete(tempPath);
            Status = $"Gagal menyimpan model: {ex.Message}";
            return;
        }
        finally
        {
            IsImportingModel = false;
            ImportStatus = string.Empty;
        }

        await RefreshModelsAsync();
        string uploadedId = Path.GetFileNameWithoutExtension(destPath);
        var uploaded = Models.FirstOrDefault(m => m.Id == uploadedId);
        if (uploaded is null)
        {
            Status = "Model tersalin tetapi tidak muncul di daftar. Coba segarkan.";
            return;
        }

        if (IsBusy)
        {
            Status = "Model tersimpan. Selesaikan analisis berjalan sebelum mengganti model.";
            return;
        }

        _suppressModelReload = true;
        try
        {
            SelectedModel = uploaded;
            Settings.ModelId = uploaded.Id;
            _settingsService.Save(Settings);
        }
        finally
        {
            _suppressModelReload = false;
        }
        await SwitchModelAsync(uploaded);
    }

    private void CopyFileWithProgress(string sourcePath, string destinationPath, string displayName)
    {
        const int bufferSize = 1024 * 1024;
        using var source = new FileStream(
            sourcePath, FileMode.Open, FileAccess.Read, FileShare.Read, bufferSize, useAsync: false);
        using var dest = new FileStream(
            destinationPath, FileMode.CreateNew, FileAccess.Write, FileShare.None, bufferSize, useAsync: false);
        var buffer = new byte[bufferSize];
        long total = source.Length;
        long copied = 0;
        int read;
        while ((read = source.Read(buffer, 0, buffer.Length)) > 0)
        {
            dest.Write(buffer, 0, read);
            copied += read;
            int pct = total > 0 ? (int)(copied * 100 / total) : 0;
            Dispatcher.UIThread.Post(() => ImportStatus = $"Menyalin {displayName}: {pct}%");
        }
        dest.Flush(flushToDisk: true);
    }

    private static string SanitizeModelFileName(string fileName)
    {
        string stem = Path.GetFileNameWithoutExtension(fileName);
        var invalid = Path.GetInvalidFileNameChars();
        var cleaned = new string(stem
            .Select(ch => invalid.Contains(ch) || char.IsControl(ch) ? '-' : ch)
            .ToArray())
            .Trim()
            .Trim('.');
        if (string.IsNullOrWhiteSpace(cleaned))
        {
            cleaned = $"model-{DateTime.UtcNow:yyyyMMddHHmmss}";
        }
        if (cleaned.Length > 120)
        {
            cleaned = cleaned[..120];
        }
        return cleaned + ".gguf";
    }

    private string UniqueModelPath(string fileName)
    {
        string stem = Path.GetFileNameWithoutExtension(fileName);
        string candidate = Path.Combine(_modelDirectory, fileName);
        int suffix = 2;
        while (File.Exists(candidate))
        {
            candidate = Path.Combine(_modelDirectory, $"{stem}-{suffix}.gguf");
            suffix++;
        }
        return candidate;
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch (IOException)
        {
            // Best effort: the partial file is ignored by the model list.
        }
        catch (UnauthorizedAccessException)
        {
            // Same as above.
        }
    }

    [RelayCommand]
    private async Task DeleteModelAsync(ModelInfo? model)
    {
        if (model is null || IsBusy || IsModelLoading || IsImportingModel || !model.Installed)
        {
            return;
        }

        await _modelGate.WaitAsync();
        try
        {
            _engine.Dispose();
            File.Delete(model.Path);
            if (string.Equals(Settings.ModelId, model.Id, StringComparison.Ordinal))
            {
                Settings.ModelId = string.Empty;
                _settingsService.Save(Settings);
            }
            _engine = CreateEngine();
        }
        catch (Exception ex)
        {
            Status = $"Gagal menghapus model: {ex.Message}";
            // The engine was disposed before the delete attempt; always restore
            // a usable handle so the app does not continue with a dead pointer.
            _engine = CreateEngine();
            return;
        }
        finally
        {
            _modelGate.Release();
        }

        Status = $"Model {model.Name} dihapus.";
        await RefreshModelsAsync();
        if (SelectedModel is { Installed: true, Compatible: true } next)
        {
            await SwitchModelAsync(next);
        }
    }

    [RelayCommand]
    private void RevealModelsFolder()
    {
        try
        {
            Directory.CreateDirectory(_modelDirectory);
            Process.Start(new ProcessStartInfo
            {
                FileName = _modelDirectory,
                UseShellExecute = true,
            });
        }
        catch (Exception ex)
        {
            Status = $"Gagal membuka folder model: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task PreloadModelAsync()
    {
        if (SelectedModel is { Installed: true, Compatible: true } model)
        {
            await SwitchModelAsync(model);
        }
        else
        {
            Status = "Pilih model yang kompatibel terlebih dahulu.";
        }
    }

    partial void OnSelectedModelChanged(ModelInfo? value)
    {
        if (value is null)
        {
            HasUsableModel = false;
            return;
        }

        ApplyModelSummary(value);
        if (_suppressModelReload)
        {
            return;
        }
        if (IsImportingModel)
        {
            // The engine handle may be disposed while the import inspects or
            // copies a file; apply the new selection after the import finishes.
            return;
        }
        if (!value.Compatible)
        {
            HasUsableModel = false;
            ModelLoadStatus = "Model ini tidak dapat dipakai untuk analisis.";
            Status = $"Model {value.Name} tidak didukung; pilih model generatif lain.";
            var fallback = Models.FirstOrDefault(m => m.Installed && m.Compatible);
            if (fallback is not null && !ReferenceEquals(fallback, value))
            {
                _suppressModelReload = true;
                try
                {
                    SelectedModel = fallback;
                }
                finally
                {
                    _suppressModelReload = false;
                }
            }
            return;
        }

        Settings.ModelId = value.Id;
        _settingsService.Save(Settings);
        if (IsBusy)
        {
            ModelLoadStatus = "Analisis sedang berjalan; model baru diterapkan setelah selesai.";
            return;
        }
        _ = SwitchModelAsync(value);
    }

    private async Task SwitchModelAsync(ModelInfo model)
    {
        int generation = Interlocked.Increment(ref _engineGeneration);
        HasUsableModel = false;
        IsModelLoading = true;
        ModelLoadStatus = "Memuat model ke RAM...";

        await _modelGate.WaitAsync();
        try
        {
            if (generation != _engineGeneration)
            {
                return;
            }

            var previous = _engine;
            _engine = CreateEngine();
            previous.Dispose();

            await Task.Run(() => _engine.PreloadModel(ev =>
            {
                Dispatcher.UIThread.Post(() =>
                {
                    if (ev.Stage is "load_model" or "model_ready")
                    {
                        ModelLoadStatus = ev.Message;
                    }
                });
            }));

            if (generation != _engineGeneration)
            {
                return;
            }

            HasUsableModel = true;
            ModelLoadStatus = $"Model siap · {model.ContextDisplay}";
            Status = $"Model aktif: {model.Name}.";
        }
        catch (Exception ex)
        {
            HasUsableModel = false;
            ModelLoadStatus = $"Gagal memuat model: {ex.Message}";
            Status = $"Model tidak dapat dipakai: {ex.Message}";
        }
        finally
        {
            if (generation == _engineGeneration)
            {
                IsModelLoading = false;
            }
            _modelGate.Release();
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
        Status = "Pengaturan disimpan.";
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
        _modelGate.Dispose();
        PosterPreview = null;
    }
}
