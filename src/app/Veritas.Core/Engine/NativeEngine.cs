using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Veritas.Core.Models;

namespace Veritas.Core.Engine;

/// <summary>
/// Progress callback invoked by the native engine during analysis or download.
/// </summary>
public delegate void ProgressCallback(string jsonEvent, IntPtr userData);

/// <summary>
/// Managed wrapper around the native veritas_engine library.
/// </summary>
public sealed class NativeEngine : IDisposable
{
    private IntPtr _handle;
    private bool _disposed;

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr veritas_init(IntPtr configJson);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern void veritas_free(IntPtr engine);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr veritas_analyze(
        IntPtr engine,
        IntPtr requestJson,
        [MarshalAs(UnmanagedType.FunctionPtr)] ProgressCallback? progressCb,
        IntPtr userData);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern void veritas_cancel(IntPtr engine, ulong requestId);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern void veritas_free_string(IntPtr s);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr veritas_health(IntPtr engine);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr veritas_list_models(IntPtr engine);

    [DllImport("veritas_engine", CallingConvention = CallingConvention.Cdecl)]
    private static extern int veritas_download_model(
        IntPtr engine,
        IntPtr modelId,
        [MarshalAs(UnmanagedType.FunctionPtr)] ProgressCallback? progressCb,
        IntPtr userData);

    public NativeEngine(string? configJson = null)
    {
        IntPtr cfg = configJson is null ? IntPtr.Zero : Marshal.StringToCoTaskMemUTF8(configJson);
        try
        {
            _handle = veritas_init(cfg);
        }
        finally
        {
            if (cfg != IntPtr.Zero)
            {
                Marshal.FreeCoTaskMem(cfg);
            }
        }
        if (_handle == IntPtr.Zero)
        {
            throw new InvalidOperationException("Gagal menginisialisasi engine native.");
        }
    }

    public AnalyzeResponse Analyze(AnalyzeRequest request, Action<ProgressEvent>? onProgress = null)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        string reqJson = JsonSerializer.Serialize(request);
        IntPtr reqPtr = Marshal.StringToCoTaskMemUTF8(reqJson);
        IntPtr resultPtr = IntPtr.Zero;
        try
        {
            var callback = CreateProgressCallback(onProgress);
            resultPtr = veritas_analyze(_handle, reqPtr, callback, IntPtr.Zero);
            if (resultPtr == IntPtr.Zero)
            {
                throw new InvalidOperationException("Analyze native mengembalikan null.");
            }

            string resultJson = Marshal.PtrToStringUTF8(resultPtr)!;
            using var doc = JsonDocument.Parse(resultJson);
            if (doc.RootElement.TryGetProperty("code", out _))
            {
                var err = JsonSerializer.Deserialize<ErrorPayload>(resultJson)!;
                throw new InvalidOperationException($"{err.Code}: {err.Message}");
            }

            return JsonSerializer.Deserialize<AnalyzeResponse>(resultJson)
                ?? throw new InvalidOperationException("Respons analisis tidak dapat dibaca.");
        }
        finally
        {
            if (resultPtr != IntPtr.Zero)
            {
                veritas_free_string(resultPtr);
            }
            Marshal.FreeCoTaskMem(reqPtr);
        }
    }

    /// <summary>
    /// Downloads and verifies a model. Blocking; call from a worker thread.
    /// Returns true on success, false when cancelled.
    /// </summary>
    public bool DownloadModel(string modelId, Action<ProgressEvent>? onProgress = null)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        IntPtr idPtr = Marshal.StringToCoTaskMemUTF8(modelId);
        try
        {
            var callback = CreateProgressCallback(onProgress);
            int code = veritas_download_model(_handle, idPtr, callback, IntPtr.Zero);
            if (code == 2)
            {
                return false;
            }
            if (code != 0)
            {
                throw new InvalidOperationException("Unduhan model gagal. Lihat pesan progres untuk detail.");
            }
            return true;
        }
        finally
        {
            Marshal.FreeCoTaskMem(idPtr);
        }
    }

    public IReadOnlyList<ModelInfo> ListModels()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        IntPtr p = veritas_list_models(_handle);
        if (p == IntPtr.Zero)
        {
            return Array.Empty<ModelInfo>();
        }
        try
        {
            string json = Marshal.PtrToStringUTF8(p) ?? "[]";
            return JsonSerializer.Deserialize<ModelInfo[]>(json) ?? Array.Empty<ModelInfo>();
        }
        finally
        {
            veritas_free_string(p);
        }
    }

    public string Health()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        IntPtr p = veritas_health(_handle);
        if (p == IntPtr.Zero)
        {
            return "{}";
        }
        try
        {
            return Marshal.PtrToStringUTF8(p) ?? "{}";
        }
        finally
        {
            veritas_free_string(p);
        }
    }

    public void Cancel(ulong requestId = 0)
    {
        if (_disposed)
        {
            return;
        }
        veritas_cancel(_handle, requestId);
    }

    private static ProgressCallback? CreateProgressCallback(Action<ProgressEvent>? onProgress)
    {
        if (onProgress is null)
        {
            return null;
        }
        return (json, _) =>
        {
            try
            {
                var ev = JsonSerializer.Deserialize<ProgressEvent>(json);
                if (ev is not null)
                {
                    onProgress(ev);
                }
            }
            catch (JsonException)
            {
                // Malformed progress events are non-fatal.
            }
        };
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        if (_handle != IntPtr.Zero)
        {
            veritas_free(_handle);
            _handle = IntPtr.Zero;
        }
    }
}
