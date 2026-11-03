using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading.Tasks;

#if WINDOWS
using Windows.Graphics.Imaging;
using Windows.Media.Ocr;
using Windows.Storage;
#endif

namespace Veritas.App.Services;

/// <summary>
/// Extracts text from a poster image. Windows OCR is tried first; Tesseract
/// (if installed) is used as a fallback because it handles stylised and
/// low-resolution text better.
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
        string best = string.Empty;
        Exception? lastError = null;

        try
        {
            best = await TryWindowsOcrAsync(path);
        }
        catch (Exception ex)
        {
            lastError = ex;
        }

        if (string.IsNullOrWhiteSpace(best))
        {
            try
            {
                string tess = await TryTesseractAsync(path);
                if (tess.Length > best.Length)
                {
                    best = tess;
                }
            }
            catch (Exception ex)
            {
                lastError = ex;
            }
        }

        if (string.IsNullOrWhiteSpace(best))
        {
            throw new InvalidOperationException(
                lastError is null
                    ? "Tidak ada teks yang terbaca. Coba gambar yang lebih jelas, atau tempel teks manual."
                    : $"OCR gagal: {lastError.Message}. Coba gambar yang lebih jelas, atau tempel teks manual.");
        }

        return best;
    }

    private static async Task<string> TryWindowsOcrAsync(string path)
    {
        var file = await StorageFile.GetFileFromPathAsync(path);
        using var stream = await file.OpenAsync(FileAccessMode.Read);
        var decoder = await BitmapDecoder.CreateAsync(stream);

        var scales = new[] { 1, 2, 3 }
            .Where(factor => decoder.PixelWidth * factor <= 4096)
            .Distinct()
            .ToArray();
        if (scales.Length == 0)
        {
            scales = new[] { 1 };
        }

        var candidateLanguages = OcrEngine.AvailableRecognizerLanguages
            .Where(language => language.LanguageTag.StartsWith("id", StringComparison.OrdinalIgnoreCase)
                            || language.LanguageTag.StartsWith("en", StringComparison.OrdinalIgnoreCase))
            .ToList();
        if (candidateLanguages.Count == 0)
        {
            candidateLanguages = OcrEngine.AvailableRecognizerLanguages.ToList();
        }

        string best = string.Empty;
        foreach (var scale in scales)
        {
            using var bitmap = await DecodeAsync(decoder, scale);
            foreach (var language in candidateLanguages)
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

            var profileEngine = OcrEngine.TryCreateFromUserProfileLanguages();
            if (profileEngine is not null)
            {
                var result = await profileEngine.RecognizeAsync(bitmap);
                var text = string.Join('\n', result.Lines.Select(line => line.Text)).Trim();
                if (text.Length > best.Length)
                {
                    best = text;
                }
            }
        }

        return best;
    }

    private static async Task<string> TryTesseractAsync(string path)
    {
        string? exe = FindTesseract();
        if (exe is null)
        {
            return string.Empty;
        }

        var psi = new ProcessStartInfo
        {
            FileName = exe,
            Arguments = $"\"{path}\" stdout -l ind+eng --psm 6",
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        using var process = Process.Start(psi);
        if (process is null)
        {
            return string.Empty;
        }

        string output = await process.StandardOutput.ReadToEndAsync();
        await process.WaitForExitAsync();
        return output.Trim();
    }

    private static string? FindTesseract()
    {
        string[] candidates =
        {
            @"C:\Program Files\Tesseract-OCR\tesseract.exe",
            @"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
        };
        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }

        try
        {
            var path = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            foreach (var dir in path.Split(Path.PathSeparator))
            {
                var full = Path.Combine(dir, "tesseract.exe");
                if (File.Exists(full))
                {
                    return full;
                }
            }
        }
        catch
        {
            // Ignore PATH lookup failures.
        }

        return null;
    }

    private static async Task<SoftwareBitmap> DecodeAsync(BitmapDecoder decoder, int scale)
    {
        var transform = new BitmapTransform();
        if (scale > 1)
        {
            transform.ScaledWidth = (uint)(decoder.PixelWidth * scale);
            transform.ScaledHeight = (uint)(decoder.PixelHeight * scale);
            transform.InterpolationMode = BitmapInterpolationMode.Fant;
        }

        return await decoder.GetSoftwareBitmapAsync(
            BitmapPixelFormat.Bgra8,
            BitmapAlphaMode.Premultiplied,
            transform,
            ExifOrientationMode.RespectExifOrientation,
            ColorManagementMode.DoNotColorManage);
    }
#endif
}