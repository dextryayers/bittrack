namespace Veritas.Core.Config;

public sealed class AppSettings
{
    public string ModelId { get; set; } = "qwen2.5-3b-q4";
    public double Threshold { get; set; } = 0.5;
    public string Language { get; set; } = "id";
    public string Theme { get; set; } = "system";
    public bool TelemetryEnabled { get; set; } = false;
}
