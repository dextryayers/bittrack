using System;
using System.Linq;
using System.Threading.Tasks;

#if WINDOWS
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

        // Small posters are the common case for screenshots and chat forwards.
        // Upscaling before OCR materially improves character recognition on
        // low-resolution images without changing the source file.
        var transform = new BitmapTransform();
        if (decoder.PixelWidth > 0 && decoder.PixelWidth < 1500)
        {
            transform.ScaledWidth = decoder.PixelWidth * 2;
            transform.ScaledHeight = decoder.PixelHeight * 2;
            transform.InterpolationMode = BitmapInterpolationMode.Fant;
        }

        using var bitmap = await decoder.GetSoftwareBitmapAsync(
            BitmapPixelFormat.Bgra8,
            BitmapAlphaMode.Premultiplied,
            transform,
            ExifOrientationMode.RespectExifOrientation,
            ColorManagementMode.DoNotColorManage);

        // Try every recognizer that can plausibly read the poster (Indonesian
        // first, then English) and keep the richest result. A single recognizer
        // often drops text that another one catches.
        var candidates = OcrEngine.AvailableRecognizerLanguages
            .Where(language => language.LanguageTag.StartsWith("id", StringComparison.OrdinalIgnoreCase)
                            || language.LanguageTag.StartsWith("en", StringComparison.OrdinalIgnoreCase))
            .ToList();

        string best = string.Empty;
        foreach (var language in candidates)
        {
            var engine = OcrEngine.TryCreateFromLanguage(language);
            if (engine is null)
            {
                continue;
            }
            var result = await engine.RecognizeAsync(bitmap);
            var text = string.Join('\n', result.Lines.Select(line => line.Text)).Trim();
            if (text.Length > best.Length)
            {
                best = text;
            }
        }

        if (string.IsNullOrWhiteSpace(best))
        {
            var fallback = OcrEngine.TryCreateFromUserProfileLanguages();
            if (fallback is not null)
            {
                var result = await fallback.RecognizeAsync(bitmap);
                best = string.Join('\n', result.Lines.Select(line => line.Text)).Trim();
            }
        }

        if (string.IsNullOrWhiteSpace(best))
        {
            throw new InvalidOperationException(
                "Windows OCR tidak menemukan teks. Coba gambar yang lebih jelas atau tempel teks manual.");
        }

        return best;
    }
#endif
}