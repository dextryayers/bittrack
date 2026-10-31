using System;
using System.Threading.Tasks;

#if WINDOWS
using System.Linq;
using Windows.Graphics.Imaging;
using Windows.Media.Ocr;
using Windows.Storage;
#endif

namespace Veritas.App.Services;

/// <summary>
/// Extracts text from a poster image using the OCR engine that ships with
/// Windows. No network access and no third-party OCR runtime are involved.
/// </summary>
public static class PosterOcrService
{
    public static Task<string> ExtractTextAsync(string path)
    {
#if WINDOWS
        return ExtractWindowsAsync(path);
#else
        return Task.FromException<string>(new PlatformNotSupportedException(
            "Analisis poster memerlukan Windows 10 versi 2004 (build 19041) atau lebih baru."));
#endif
    }

#if WINDOWS
    private static async Task<string> ExtractWindowsAsync(string path)
    {
        var file = await StorageFile.GetFileFromPathAsync(path);
        using var stream = await file.OpenAsync(FileAccessMode.Read);
        var decoder = await BitmapDecoder.CreateAsync(stream);
        using var bitmap = await decoder.GetSoftwareBitmapAsync();

        var indonesian = OcrEngine.AvailableRecognizerLanguages
            .FirstOrDefault(l => l.LanguageTag.StartsWith("id", StringComparison.OrdinalIgnoreCase));
        var engine = (indonesian is not null ? OcrEngine.TryCreateFromLanguage(indonesian) : null)
                     ?? OcrEngine.TryCreateFromUserProfileLanguages();
        if (engine is null)
        {
            throw new InvalidOperationException(
                "Windows OCR tidak tersedia. Pasang komponen OCR melalui Settings > Apps > Optional features.");
        }

        var result = await engine.RecognizeAsync(bitmap);
        return string.Join('\n', result.Lines.Select(line => line.Text)).Trim();
    }
#endif
}
