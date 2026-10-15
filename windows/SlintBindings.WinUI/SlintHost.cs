// C# owner of one embedded Slint component. UI-thread only, like the Swift SlintHost.

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace SlintBindings.WinUI;

public sealed class SlintException(string message) : Exception(message);

/// <summary>A <c>.slint</c> file compiled at run time: its main (last exported) component.</summary>
public sealed class SlintDefinition : IDisposable
{
    internal IntPtr Handle { get; private set; }

    /// <summary>Compiles <paramref name="source"/>, or the file at <paramref name="path"/> when it is null.</summary>
    public SlintDefinition(string path, string? source = null)
    {
        Handle = NativeMethods.sb_compile(source, path);
        if (Handle == IntPtr.Zero) throw new SlintException(NativeMethods.LastError());
    }

    public void Dispose()
    {
        NativeMethods.sb_definition_free(Handle);
        Handle = IntPtr.Zero;
    }
}

public sealed class SlintHost : IDisposable
{
    // One thunk for every callback; a static field keeps it alive while Rust holds its pointer.
    private static readonly SbCallbackFn CallbackThunk = OnCallbackNative;
    private IntPtr _handle;
    private readonly Dictionary<string, GCHandle> _callbacks = new();

    public int PixelWidth { get; private set; } = 1;
    public int PixelHeight { get; private set; } = 1;
    public bool IsGpu { get; }

    public SlintHost(SlintDefinition definition, int pixelWidth, int pixelHeight, float scale)
        : this(definition, pixelWidth, pixelHeight, scale, IntPtr.Zero)
    {
    }

    /// <summary>
    /// GPU host. <paramref name="swapChainPanel"/> is an <c>ISwapChainPanel</c>
    /// that must stay alive until <see cref="Dispose"/>.
    /// </summary>
    public SlintHost(SlintDefinition definition, int pixelWidth, int pixelHeight, float scale, IntPtr swapChainPanel)
        : this(swapChainPanel == IntPtr.Zero
            ? NativeMethods.sb_component_new(definition.Handle, (uint)Math.Max(pixelWidth, 1), (uint)Math.Max(pixelHeight, 1), scale)
            : NativeMethods.sb_component_new_swapchain(
                definition.Handle, swapChainPanel, (uint)Math.Max(pixelWidth, 1), (uint)Math.Max(pixelHeight, 1), scale))
    {
    }

    /// <summary>
    /// Takes ownership of an <c>SbHost*</c> that another library made, such as
    /// weft's <c>ws_surface_new</c>. Zero throws the library's last error.
    /// </summary>
    public static SlintHost Adopt(IntPtr handle) => new(handle);

    private unsafe SlintHost(IntPtr handle)
    {
        if (handle == IntPtr.Zero) throw new SlintException(NativeMethods.LastError());
        uint width, height;
        if (!NativeMethods.sb_host_pixel_size(handle, &width, &height))
        {
            var error = NativeMethods.LastError();
            NativeMethods.sb_host_free(handle);
            throw new SlintException(error);
        }
        // The size comes from the host: an adopted one was sized by code that never told C#.
        _handle = handle;
        IsGpu = NativeMethods.sb_host_is_gpu(handle);
        PixelWidth = (int)width;
        PixelHeight = (int)height;
    }

    private static void OnCallbackNative(IntPtr userData, IntPtr argsJson)
    {
        if (GCHandle.FromIntPtr(userData).Target is Action<string> handler)
            handler(Marshal.PtrToStringUTF8(argsJson) ?? "[]");
    }

    // Properties and callbacks as JSON values: "\"text\"", "42", "true", "\"#ff0000\"",
    // arrays for models, objects for structs.

    public void SetProperty(string name, string json)
    {
        if (!NativeMethods.sb_set_property_json(_handle, name, json))
            throw new SlintException(NativeMethods.LastError());
    }

    public string GetProperty(string name) => TakeString(NativeMethods.sb_get_property_json(_handle, name));

    /// <summary>Calls the callback <paramref name="name"/> with a JSON array; returns its JSON result.</summary>
    public string Invoke(string name, string argsJson = "[]") =>
        TakeString(NativeMethods.sb_invoke_json(_handle, name, argsJson));

    /// <summary>Handles the callback <paramref name="name"/>; the handler gets its arguments as a JSON array.</summary>
    public void OnCallback(string name, Action<string> handler)
    {
        var box = GCHandle.Alloc(handler);
        if (!NativeMethods.sb_on_callback(_handle, name, Marshal.GetFunctionPointerForDelegate(CallbackThunk), GCHandle.ToIntPtr(box)))
        {
            box.Free();
            throw new SlintException(NativeMethods.LastError());
        }
        // The replaced closure, the only holder of the old handle, is gone now.
        if (_callbacks.Remove(name, out var old)) old.Free();
        _callbacks[name] = box;
    }

    private static string TakeString(IntPtr text)
    {
        if (text == IntPtr.Zero) throw new SlintException(NativeMethods.LastError());
        try { return Marshal.PtrToStringUTF8(text) ?? ""; }
        finally { NativeMethods.sb_string_free(text); }
    }

    public static void Tick() => NativeMethods.sb_tick();

    public void Resize(int pixelWidth, int pixelHeight, float scale)
    {
        var width = Math.Max(pixelWidth, 1);
        var height = Math.Max(pixelHeight, 1);
        if (!NativeMethods.sb_host_resize(_handle, (uint)width, (uint)height, scale))
            throw new SlintException(NativeMethods.LastError());
        // Only publish the size after the core accepts it, so a failed call leaves the previous frame.
        PixelWidth = width;
        PixelHeight = height;
    }

    /// <summary>Renders if the scene changed and writes premultiplied BGRA8 into <paramref name="bgra"/>.</summary>
    /// <returns>True when <paramref name="bgra"/> was written.</returns>
    public unsafe bool RenderBgra(Span<byte> bgra, out bool animating)
    {
        var len = (int)NativeMethods.sb_host_frame_len(_handle);
        if (bgra.Length < len)
            throw new SlintException("BGRA buffer is shorter than the frame");
        SbFrame frame;
        fixed (byte* p = bgra)
            frame = NativeMethods.sb_host_render_bgra(_handle, p, (nuint)bgra.Length);
        if (frame.Failed != 0) throw new SlintException(NativeMethods.LastError());
        animating = frame.Animating != 0;
        return frame.Redrawn != 0;
    }

    /// <summary>Presents one frame into the swap chain. True when a new frame was drawn.</summary>
    public bool RenderGpu(out bool animating)
    {
        var frame = NativeMethods.sb_host_gpu_render(_handle);
        if (frame.Failed != 0) throw new SlintException(NativeMethods.LastError());
        animating = frame.Animating != 0;
        return frame.Redrawn != 0;
    }

    public void PointerMoved(float x, float y) => NativeMethods.sb_host_pointer_moved(_handle, x, y);
    public void PointerPressed(float x, float y, int button)
    {
        if (button is < 0 or > 2) throw new SlintException("pointer button must be 0, 1 or 2");
        NativeMethods.sb_host_pointer_pressed(_handle, x, y, (byte)button);
    }
    public void PointerReleased(float x, float y, int button)
    {
        if (button is < 0 or > 2) throw new SlintException("pointer button must be 0, 1 or 2");
        NativeMethods.sb_host_pointer_released(_handle, x, y, (byte)button);
    }
    public void PointerExited() => NativeMethods.sb_host_pointer_exited(_handle);
    public void PointerScrolled(float x, float y, float dx, float dy) => NativeMethods.sb_host_pointer_scrolled(_handle, x, y, dx, dy);
    /// <returns>False when Slint rejected the key (Tab with nowhere to go) or the call failed.</returns>
    public bool KeyPressed(string text) => NativeMethods.sb_host_key_pressed(_handle, text);
    public void KeyReleased(string text) => NativeMethods.sb_host_key_released(_handle, text);
    public void KeyRepeated(string text) => NativeMethods.sb_host_key_repeated(_handle, text);
    public void FocusChanged(bool focused) => NativeMethods.sb_host_focus_changed(_handle, focused);

    public bool ImeStarted() => NativeMethods.sb_host_ime_started(_handle);
    public bool ImeComposing() => NativeMethods.sb_host_ime_composing(_handle);
    public bool ImeReplace(int rangeStart, int rangeEnd, string text, int selStart, int selEnd) =>
        NativeMethods.sb_host_ime_replace(_handle, rangeStart, rangeEnd, text, selStart, selEnd);
    public bool ImeSelect(int start, int end) => NativeMethods.sb_host_ime_select(_handle, start, end);
    public bool ImeCompleted(bool canceled) => NativeMethods.sb_host_ime_completed(_handle, canceled);

    public unsafe string ImeText()
    {
        Span<byte> buffer = stackalloc byte[4096];
        fixed (byte* p = buffer)
        {
            if (!NativeMethods.sb_host_ime_text(_handle, p, (nuint)buffer.Length))
                return "";
        }
        return Utf8(buffer);
    }

    public unsafe (int Start, int End) ImeSelection()
    {
        int start, end;
        if (!NativeMethods.sb_host_ime_selection(_handle, &start, &end))
            return (0, 0);
        return (start, end);
    }

    private static string Utf8(ReadOnlySpan<byte> buffer)
    {
        var n = buffer.IndexOf((byte)0);
        if (n < 0) n = buffer.Length;
        return Encoding.UTF8.GetString(buffer[..n]);
    }

    public void Dispose()
    {
        if (_handle == IntPtr.Zero) return;
        // Clear the callbacks first so Rust never calls into a freed handle.
        foreach (var (name, box) in _callbacks)
        {
            NativeMethods.sb_on_callback(_handle, name, IntPtr.Zero, IntPtr.Zero);
            box.Free();
        }
        _callbacks.Clear();
        NativeMethods.sb_host_free(_handle);
        _handle = IntPtr.Zero;
    }
}
