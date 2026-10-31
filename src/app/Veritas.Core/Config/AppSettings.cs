using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace Veritas.Core.Config;

public sealed class AppSettings : INotifyPropertyChanged
{
    private string _modelId = "qwen2.5-3b-instruct-q4_k_m";
    private double _threshold = 0.5;
    private string _language = "id";
    private string _theme = "system";
    private bool _telemetryEnabled;
    private bool _onlineEvidence = true;

    public string ModelId
    {
        get => _modelId;
        set => Set(ref _modelId, value);
    }

    public double Threshold
    {
        get => _threshold;
        set => Set(ref _threshold, value);
    }

    public string Language
    {
        get => _language;
        set => Set(ref _language, value);
    }

    public string Theme
    {
        get => _theme;
        set => Set(ref _theme, value);
    }

    public bool TelemetryEnabled
    {
        get => _telemetryEnabled;
        set => Set(ref _telemetryEnabled, value);
    }

    public bool OnlineEvidence
    {
        get => _onlineEvidence;
        set => Set(ref _onlineEvidence, value);
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private void Set<T>(ref T field, T value, [CallerMemberName] string? propertyName = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value))
        {
            return;
        }
        field = value;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}
