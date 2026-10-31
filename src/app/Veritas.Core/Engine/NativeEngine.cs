using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Veritas.Core.Models;

namespace Veritas.Core.Engine;

/// <summary>
/// Progress callback invoked by the native engine during analysis.
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
            throw new InvalidOperationException("Failed to initialize native engine.");
        }
    }

    public Models.AnalyzeResponse Analyze(Models.AnalyzeRequest request, Action<Models.ProgressEvent>? onProgress = null)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        string reqJson = JsonSerializer.Serialize(request);
        IntPtr reqPtr = Marshal.StringToCoTaskMemUTF8(reqJson);
        IntPtr resultPtr = IntPtr.Zero;
        try
        {
            ProgressCallback? cb = null;
            if (onProgress is not null)
            {
                cb = (json, _) =>
                {
                    try
                    {
                        var ev = JsonSerializer.Deserialize<Models.ProgressEvent>(json);
                        if (ev is not null)
                        {
                            onProgress(ev);
                        }
                    }
                    catch
                    {
                        // ignore malformed progress events
                    }
                };
            }

            resultPtr = veritas_analyze(_handle, reqPtr, cb, IntPtr.Zero);
            if (resultPtr == IntPtr.Zero)
            {
                throw new InvalidOperationException("Native analyze returned null.");
            }

            string resultJson = Marshal.PtrToStringUTF8(resultPtr)!;
            // Try parse as response; if it has code/message treat as error.
            using var doc = JsonDocument.Parse(resultJson);
            if (doc.RootElement.TryGetProperty("code", out _))
            {
                var err = JsonSerializer.Deserialize<Models.ErrorPayload>(resultJson)!;
                throw new InvalidOperationException($"{err.Code}: {err.Message}");
            }

            return JsonSerializer.Deserialize<Models.AnalyzeResponse>(resultJson)!;
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
