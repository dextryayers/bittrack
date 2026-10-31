using System.Text.Json;

namespace Veritas.Core.Services;

public sealed class SettingsService
{
    private readonly string _path;
    private readonly JsonSerializerOptions _opts = new() { WriteIndented = true };

    public SettingsService(string path)
    {
        _path = path;
        var dir = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(dir))
        {
            Directory.CreateDirectory(dir);
        }
    }

    public Veritas.Core.Config.AppSettings Load()
    {
        if (!File.Exists(_path))
        {
            return new Veritas.Core.Config.AppSettings();
        }
        try
        {
            var json = File.ReadAllText(_path);
            return JsonSerializer.Deserialize<Veritas.Core.Config.AppSettings>(json) ?? new Veritas.Core.Config.AppSettings();
        }
        catch
        {
            return new Veritas.Core.Config.AppSettings();
        }
    }

    public void Save(Veritas.Core.Config.AppSettings settings)
    {
        var json = JsonSerializer.Serialize(settings, _opts);
        File.WriteAllText(_path, json);
    }
}
