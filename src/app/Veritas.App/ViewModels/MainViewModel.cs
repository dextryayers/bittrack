using System;
using System.Collections.ObjectModel;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Veritas.Core.Engine;
using Veritas.Core.Models;

namespace Veritas.App.ViewModels;

public partial class MainViewModel : ObservableObject, IDisposable
{
    private readonly NativeEngine _engine;

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

    public ObservableCollection<Claim> ResultClaims { get; } = new();

    public MainViewModel()
    {
        _engine = new NativeEngine();
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
            var req = new AnalyzeRequest(Text: InputText, LangHint: "id");
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

    public void Dispose()
    {
        _engine.Dispose();
    }
}
